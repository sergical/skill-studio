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
# A folder is in use while Cargo holds one of its `<profile>/.cargo-lock`
# files (compiling), or while a process runs from a path inside it (test
# binaries and rustdoc during `cargo test`, which run after Cargo releases its
# locks). To empty a folder, this script takes its locks first, then checks
# the process list 20 times over 2 seconds, so the short gap between two test
# binaries cannot hide a test run. It deletes everything except the lock
# files. A Cargo that waits on a lock then gets the same lock file and builds
# from scratch. A folder seen in use is marked, and the idle window keeps it
# for an hour after each mark. Any failed check skips the folder.
#
# A stopped Cargo (Ctrl-Z, SIGSTOP, a debugger) between two test binaries runs
# nothing from the folder and holds no lock, so while any Cargo is stopped this
# script empties nothing. Known gap: a `cargo test` that the system does not
# schedule for over 2 seconds between two test binaries, in a folder not
# written for an hour, can lose its folder. That test run fails and the next
# one rebuilds.
#
#   CARGO_BUILD_GC_CAP_GB       total size cap (default 40)
#   CARGO_BUILD_GC_FLOOR_GB     free-disk floor (default 30)
#   CARGO_BUILD_GC_MAX_DAYS     empty folders not used for this long (default 3)
#   CARGO_BUILD_GC_MIN_IDLE_MIN never empty folders used more recently (default 60)
#   CARGO_BUILD_GC_DRY_RUN=1    print what would go, change nothing
use strict;
use warnings;
use Cwd qw(realpath);
use Fcntl qw(:flock O_APPEND O_CREAT O_NOFOLLOW O_WRONLY);
use File::Basename qw(dirname);
use File::Glob qw(bsd_glob);
use File::Path qw(make_path remove_tree);
use POSIX qw(strftime);

my $KB_PER_GB       = 1024 * 1024;
my $PROCESS_SAMPLES = 20;
my $cap_kb    = ($ENV{CARGO_BUILD_GC_CAP_GB}   // 40) * $KB_PER_GB;
my $floor_kb  = ($ENV{CARGO_BUILD_GC_FLOOR_GB} // 30) * $KB_PER_GB;
my $max_age   = ($ENV{CARGO_BUILD_GC_MAX_DAYS} // 3) * 86400;
my $min_idle  = ($ENV{CARGO_BUILD_GC_MIN_IDLE_MIN} // 60) * 60;
my $dry_run   = ($ENV{CARGO_BUILD_GC_DRY_RUN} // '') eq '1';

my $cargo_home = $ENV{CARGO_HOME} // "$ENV{HOME}/.cargo";
my $root       = realpath("$cargo_home/build");
exit 0 unless defined $root && -d $root;
my $seen = "$root/.gc-last-busy";

sub logline { print strftime('%F %T', localtime), " @_\n" }

if (-l $seen || (-e _ && !-d _)) {
    logline("stop: $seen is not a plain folder, so no folder is emptied");
    exit 1;
}

sub open_nofollow { sysopen($_[0], $_[1], O_WRONLY | O_APPEND | O_CREAT | O_NOFOLLOW) }

sub output_of {
    open(my $out, '-|', @_) or return ();
    my @lines = <$out>;
    close($out) or return ();
    return @lines;
}

open_nofollow(my $instance, "$root/.gc.lock") or die "cannot open $root/.gc.lock: $!\n";
exit 0 unless flock($instance, LOCK_EX | LOCK_NB);

sub glob_in {
    my ($dir, @patterns) = @_;
    (my $quoted = $dir) =~ s/([\\\[\]{}*?~])/\\$1/g;
    return map { bsd_glob("$quoted/$_") } @patterns;
}

# True when $dir has no symlink in any part of its path below $ws.
sub inside {
    my ($ws, $dir) = @_;
    my $real = realpath($dir);
    return defined $real && $real eq $dir && index($real, "$ws/") == 0;
}

# Returns exclusively locked handles for every Cargo lock in the folder, or
# undef when Cargo holds one of them.
sub lock_all {
    my ($ws) = @_;
    my @held;
    for my $path (glob_in($ws, '*/.cargo-lock', '*/*/.cargo-lock')) {
        my $dir = dirname($path);
        next if -l $path || !inside($ws, $dir);
        open(my $fh, '<', $path) or return undef;
        flock($fh, LOCK_EX | LOCK_NB) or return undef;
        push @held, { fh => $fh, dir => $dir };
    }
    return \@held;
}

# Matches on `/<xx>/<hash>/`, so a process started through a symlinked path to
# the folder still counts.
sub runs_from {
    my ($id, $samples) = @_;
    for my $n (1 .. $samples) {
        select(undef, undef, undef, 0.1) if $n > 1;
        my @commands = `ps -ax -ww -o command=`;
        return 1 if $?;
        return 1 if grep { index($_, "/$id/") >= 0 } @commands;
    }
    return 0;
}

sub cargo_stopped {
    my @procs = `ps -ax -o stat=,ucomm=`;
    return 1 if $?;
    return scalar grep { /^\s*T\S*\s+cargo\s*$/ } @procs;
}

sub mtime { my @s = lstat($_[0]); return @s ? $s[9] : 0 }

sub last_used {
    my ($ws, $mark) = @_;
    my $newest = mtime("$seen/$mark");
    for my $p ($ws, glob_in($ws, qw(*/deps */.fingerprint */*/deps */*/.fingerprint))) {
        my $t = mtime($p);
        $newest = $t if $t > $newest;
    }
    return $newest;
}

sub size_kb {
    my ($out) = output_of('du', '-sk', $_[0]);
    return (defined $out && $out =~ /^(\d+)/) ? $1 : undef;
}

sub free_kb {
    my @lines = output_of('df', '-k', $root);
    my @cols  = split ' ', ($lines[1] // '');
    die "df failed for $root\n" unless defined $cols[3] && $cols[3] =~ /^\d+$/;
    return $cols[3];
}

sub mark_busy {
    my ($mark) = @_;
    return if $dry_run;
    make_path($seen);
    open_nofollow(my $fh, "$seen/$mark") or return;
    utime(undef, undef, $fh);
    close($fh);
}

my $now = time;
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
        my $id = "$shard/$name";
        mark_busy("$shard$name") if !lock_all($ws) || runs_from($id, 1);
        push @workspaces,
            { ws => $ws, id => $id, mark => "$shard$name", used => last_used($ws, "$shard$name"), kb => $kb };
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
    my $busy  = !$locks                                  ? 'Cargo holds its lock'
              : runs_from($w->{id}, $PROCESS_SAMPLES)    ? 'a process runs from it'
              :                                            '';
    if ($busy) {
        mark_busy($w->{mark});
        logline("skip $w->{ws} ($why): $busy");
        next;
    }
    if (cargo_stopped()) {
        logline('stop: a Cargo process is stopped, so no folder is emptied in this run');
        last;
    }
    if ($dry_run) {
        logline("would empty $w->{ws} ($why, " . int($w->{kb} / 1024) . ' MB)');
        $total -= $w->{kb};
        $freed += $w->{kb};
        next;
    }

    my @failed;
    for my $lock (@$locks) {
        next unless inside($w->{ws}, $lock->{dir});
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
    unlink("$seen/$w->{mark}");
    logline("emptied $w->{ws} ($why, " . int(($w->{kb} - $left) / 1024) . ' MB freed)'
        . (@failed ? '; could not delete: ' . join(', ', @failed) : ''));
}

my $note = $total > $cap_kb ? ' (over cap: the rest is in use or used within the idle window)' : '';
logline(sprintf('%d folders, %.1f GB%s; %.1f GB free',
    scalar(@workspaces), $total / $KB_PER_GB, $note, (free_kb() + $freed) / $KB_PER_GB)
    . ($dry_run ? ' (projected)' : ''));
