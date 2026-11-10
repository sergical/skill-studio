#!/usr/bin/perl
# Keeps Cargo build folders under a size cap and a free-disk floor.
#
# Expects `build.build-dir = "{cargo-cache-home}/build/{workspace-path-hash}"`,
# which gives each workspace (each git worktree) one folder:
#   $CARGO_HOME/build/<xx>/<hash>/
# Empties whole folders, least recently used first. Emptying one only costs a
# rebuild. This is an eviction threshold, not a hard bound: folders in use
# are never touched, so builds that run at the same time can go over the cap.
#
# A folder is in use while:
#   - Cargo holds one of its `<profile>/.cargo-lock` files (compiling),
#   - a running `cargo` has it as its build directory (found with
#     `cargo metadata` in the process's working directory, with its
#     `--manifest-path` and `CARGO_HOME`; this covers test runs, which start
#     after Cargo releases its locks; `--config`, `-Z`, `-C`, or
#     `CARGO_BUILD_BUILD_DIR` make the build directory unknown), or
#   - a process runs from a path inside it.
# To empty a folder, this script takes its locks first, checks the other two
# conditions while it holds them, and deletes everything except the lock
# files. A Cargo that waits on a lock then gets the same lock file and builds
# from scratch. Any failed check skips the folder.
#
#   CARGO_BUILD_GC_CAP_GB       total size cap (default 40)
#   CARGO_BUILD_GC_FLOOR_GB     free-disk floor (default 30)
#   CARGO_BUILD_GC_MAX_DAYS     empty folders not used for this long (default 3)
#   CARGO_BUILD_GC_MIN_IDLE_MIN never empty folders used more recently (default 60)
#   CARGO_BUILD_GC_DRY_RUN=1    print what would go, change nothing
use strict;
use warnings;
use Cwd qw(realpath);
use Fcntl qw(:flock);
use File::Basename qw(dirname);
use File::Glob qw(bsd_glob);
use File::Path qw(make_path remove_tree);
use POSIX qw(strftime);

my $KB_PER_GB = 1024 * 1024;
my $cap_kb    = ($ENV{CARGO_BUILD_GC_CAP_GB}   // 40) * $KB_PER_GB;
my $floor_kb  = ($ENV{CARGO_BUILD_GC_FLOOR_GB} // 30) * $KB_PER_GB;
my $max_age   = ($ENV{CARGO_BUILD_GC_MAX_DAYS} // 3) * 86400;
my $min_idle  = ($ENV{CARGO_BUILD_GC_MIN_IDLE_MIN} // 60) * 60;
my $dry_run   = ($ENV{CARGO_BUILD_GC_DRY_RUN} // '') eq '1';

my $cargo_home = $ENV{CARGO_HOME} // "$ENV{HOME}/.cargo";
my $cargo_bin  = "$cargo_home/bin/cargo";
my $root       = realpath("$cargo_home/build");
exit 0 unless defined $root && -d $root;
my $seen = "$root/.gc-last-busy";

sub logline { print strftime('%F %T', localtime), " @_\n" }

open(my $instance, '>>', "$root/.gc.lock") or die "cannot open $root/.gc.lock: $!\n";
exit 0 unless flock($instance, LOCK_EX | LOCK_NB);

# Returns exclusively locked handles for every Cargo lock in the folder, or
# undef when Cargo holds one of them.
sub lock_all {
    my ($ws) = @_;
    my @held;
    for my $path (map { bsd_glob("$ws/$_") } '*/.cargo-lock', '*/*/.cargo-lock') {
        next if -l $path;
        open(my $fh, '<', $path) or return undef;
        flock($fh, LOCK_EX | LOCK_NB) or return undef;
        push @held, { fh => $fh, dir => dirname($path) };
    }
    return \@held;
}

sub cwd_of {
    my ($pid) = @_;
    my $link = readlink("/proc/$pid/cwd");
    return $link if defined $link;
    for (`lsof -a -d cwd -p $pid -Fn 2>/dev/null`) { return $1 if /^n(.+)/ }
    return undef;
}

# Arguments and environment of a process, or an empty list when unreadable.
# macOS `ps -E` prints the environment after the arguments; it prints nothing
# for processes it may not inspect.
sub invocation {
    my ($pid) = @_;
    if (open(my $cmd, '<', "/proc/$pid/cmdline")) {
        my @args = split /\0/, do { local $/; <$cmd> };
        open(my $environ, '<', "/proc/$pid/environ") or return;
        my %env = map { /^([^=]*)=(.*)$/s ? ($1, $2) : () } split /\0/, do { local $/; <$environ> };
        return (\@args, \%env);
    }
    chomp(my $args = `ps -ww -o args= -p $pid 2>/dev/null`);
    chomp(my $all  = `ps -E -ww -o command= -p $pid 2>/dev/null`);
    return unless length($args) && index($all, "$args ") == 0;
    my $env = substr($all, length($args) + 1);
    return unless $env =~ /^[A-Za-z_]\w*=/;
    my %env;
    while ($env =~ /(?:^|\s)(CARGO_HOME|CARGO_BUILD_BUILD_DIR|HOME)=(\S*)/g) {
        my ($name, $value) = ($1, $2);
        # A value with a space would be cut short here: treat it as unknown.
        $value = undef unless substr($env, pos($env)) =~ /^(?:\s+[A-Za-z_]\w*=|\s*$)/;
        $env{$name} = $value;
    }
    return ([split ' ', $args], \%env);
}

# CARGO_HOME and `cargo metadata` arguments that select the same build
# directory as the process, or an empty list when that is unknown.
sub workspace_args {
    my ($cwd, $args, $env) = @_;
    return if exists $env->{CARGO_BUILD_BUILD_DIR};
    my $home = exists $env->{CARGO_HOME} ? $env->{CARGO_HOME}
             : defined $env->{HOME}      ? "$env->{HOME}/.cargo"
             :                             undef;
    return unless defined $home;
    $home = realpath($home =~ m{^/} ? $home : "$cwd/$home");
    return unless defined $home;
    my @extra;
    for (my $i = 1; $i < @$args; $i++) {
        my $arg = $args->[$i];
        last if $arg eq '--';
        return if $arg =~ /^(?:--config|-Z|-C)/;
        next unless $arg =~ /^--manifest-path(?:=(.*))?$/;
        my $path = $1 // $args->[++$i];
        return unless defined $path && $path =~ m{(?:^|/)Cargo\.toml$};
        push @extra, '--manifest-path', $path =~ m{^/} ? $path : "$cwd/$path";
    }
    return ($home, @extra);
}

my %build_dir_of;
sub build_dir_for {
    my ($cwd, $home, @extra) = @_;
    my $key = join("\0", $cwd, $home, @extra);
    return $build_dir_of{$key} if exists $build_dir_of{$key};
    my $pid = open(my $out, '-|');
    return undef unless defined $pid;
    if (!$pid) {
        chdir($cwd) or exit 1;
        open(STDERR, '>', '/dev/null');
        $ENV{CARGO_HOME} = $home;
        $ENV{RUSTUP_AUTO_INSTALL} = '0';
        exec($cargo_bin, qw(metadata --no-deps --format-version 1 --offline), @extra) or exit 1;
    }
    my $json = do { local $/; <$out> };
    close($out) or return $build_dir_of{$key} = undef;
    my ($dir) = ($json // '') =~ /"build_directory":"([^"\\]*)"/;
    return $build_dir_of{$key} = defined $dir ? realpath($dir) // $dir : undef;
}

# Build directories of every running cargo, or undef when one is unknown.
sub active_build_dirs {
    my %dirs;
    for my $pid (map { /(\d+)/ ? $1 : () } `pgrep -x cargo`) {
        next if $pid == $$;
        my $cwd = cwd_of($pid);
        my ($args, $env) = invocation($pid);
        next if !(defined $cwd && $args) && !kill(0, $pid);    # exited meanwhile
        return undef unless defined $cwd && $args;
        my @selection = workspace_args($cwd, $args, $env);
        return undef unless @selection;
        my $dir = build_dir_for($cwd, @selection);
        return undef unless defined $dir;
        $dirs{$dir} = 1;
    }
    return \%dirs;
}

sub runs_from {
    my ($ws) = @_;
    my @commands = `ps -ax -ww -o command=`;
    return 1 if $?;
    return scalar grep { index($_, "$ws/") >= 0 } @commands;
}

# Why the folder is in use, or '' when it is free. Call with its locks held.
sub in_use {
    my ($ws) = @_;
    my $active = active_build_dirs();
    return 'a running cargo has an unknown build directory' unless $active;
    return 'a running cargo builds here' if $active->{$ws};
    return 'a process runs from it' if runs_from($ws);
    return '';
}

sub mtime { my @s = lstat($_[0]); return @s ? $s[9] : 0 }

sub last_used {
    my ($ws, $id) = @_;
    my $newest = mtime("$seen/$id");
    for my $p ($ws, map { bsd_glob("$ws/$_") } qw(*/deps */.fingerprint */*/deps */*/.fingerprint)) {
        my $t = mtime($p);
        $newest = $t if $t > $newest;
    }
    return $newest;
}

sub size_kb {
    my $out = `du -sk "$_[0]" 2>/dev/null`;
    return ($out =~ /^(\d+)/) ? $1 : undef;
}

sub free_kb {
    my @lines = `df -k "$root"`;
    my @cols  = split ' ', ($lines[1] // '');
    die "df failed for $root\n" unless defined $cols[3] && $cols[3] =~ /^\d+$/;
    return $cols[3];
}

sub mark_busy {
    my ($id) = @_;
    return if $dry_run;
    make_path($seen);
    if (open(my $mark, '>>', "$seen/$id")) { close($mark) }
    utime(undef, undef, "$seen/$id");
}

my $now = time;
my $active_now = active_build_dirs() // {};
my @workspaces;
opendir(my $rd, $root) or die "cannot read $root: $!\n";
for my $shard (grep { /^[0-9a-f]{2}$/ } readdir($rd)) {
    my $sd = "$root/$shard";
    next if -l $sd || !-d _;
    opendir(my $dh, $sd) or next;
    for my $name (grep { /^[0-9a-f]+$/ } readdir($dh)) {
        my $ws = "$sd/$name";
        next if -l $ws || !-d _;
        my $real = realpath($ws);
        next unless defined $real && $real eq $ws;
        my $kb = size_kb($ws);
        if (!defined $kb) { logline("skip $ws: du failed"); next }
        my $id = "$shard$name";
        mark_busy($id) if !lock_all($ws) || $active_now->{$ws} || runs_from($ws);
        push @workspaces, { ws => $ws, id => $id, used => last_used($ws, $id), kb => $kb };
    }
    closedir($dh);
}
closedir($rd);

my $total = 0;
$total += $_->{kb} for @workspaces;
my $freed = 0;

for my $w (sort { $a->{used} <=> $b->{used} } @workspaces) {
    my $age = $now - $w->{used};
    next if $age < $min_idle || $w->{kb} < 1024;
    my $why =
        $age > $max_age                    ? 'not used for ' . int($age / 86400) . ' days'
      : $total > $cap_kb                   ? 'over cap'
      : free_kb() + $freed < $floor_kb     ? 'below free-disk floor'
      :                                      undef;
    next unless $why;

    my $locks = lock_all($w->{ws});
    my $busy  = $locks ? in_use($w->{ws}) : 'Cargo holds its lock';
    if ($busy) {
        logline("skip $w->{ws} ($why): $busy");
        next;
    }
    if ($dry_run) {
        logline("would empty $w->{ws} ($why, " . int($w->{kb} / 1024) . ' MB)');
        $total -= $w->{kb};
        $freed += $w->{kb};
        next;
    }

    my @failed;
    for my $lock (@$locks) {
        opendir(my $pd, $lock->{dir}) or next;
        for my $entry (grep { $_ ne '.' && $_ ne '..' && $_ ne '.cargo-lock' } readdir($pd)) {
            remove_tree("$lock->{dir}/$entry", { error => \my $err });
            push @failed, "$lock->{dir}/$entry" if $err && @$err;
        }
        closedir($pd);
    }
    undef $locks;
    my $left = size_kb($w->{ws}) // $w->{kb};
    $total -= $w->{kb} - $left;
    unlink("$seen/$w->{id}");
    logline("emptied $w->{ws} ($why, " . int(($w->{kb} - $left) / 1024) . ' MB freed)'
        . (@failed ? '; could not delete: ' . join(', ', @failed) : ''));
}

my $note = $total > $cap_kb ? ' (over cap: the rest is in use or used within the idle window)' : '';
logline(sprintf('%d folders, %.1f GB%s; %.1f GB free',
    scalar(@workspaces), $total / $KB_PER_GB, $note, (free_kb() + $freed) / $KB_PER_GB)
    . ($dry_run ? ' (projected)' : ''));
