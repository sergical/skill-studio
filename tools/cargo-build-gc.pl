#!/usr/bin/perl
# Keeps Cargo build folders under a size cap and a free-disk floor.
#
# Expects `build.build-dir = "{cargo-cache-home}/build/{workspace-path-hash}"`,
# which gives each workspace (each git worktree) one folder:
#   $CARGO_HOME/build/<xx>/<hash>/
# Deletes whole folders, least recently used first. Deleting one only costs a
# rebuild.
#
# A folder is in use while Cargo holds its `.cargo-lock` (compiling) or while
# a process runs from inside it (Cargo releases the lock before it runs test
# binaries). To delete, this script takes every lock of the folder, checks for
# processes, and renames the folder away while it holds the locks. A Cargo
# that starts later waits on the lock, then builds into a new folder.
#
#   CARGO_BUILD_GC_CAP_GB    total size cap (default 40)
#   CARGO_BUILD_GC_FLOOR_GB  free-disk floor (default 30)
#   CARGO_BUILD_GC_MAX_DAYS  delete folders not used for this long (default 3)
#   CARGO_BUILD_GC_DRY_RUN=1 print what would go, delete nothing
use strict;
use warnings;
use Cwd qw(realpath);
use Fcntl qw(:flock);
use File::Glob qw(bsd_glob);
use File::Path qw(make_path remove_tree);
use POSIX qw(strftime);

my $KB_PER_GB = 1024 * 1024;
my $cap_kb   = ($ENV{CARGO_BUILD_GC_CAP_GB}   // 40) * $KB_PER_GB;
my $floor_kb = ($ENV{CARGO_BUILD_GC_FLOOR_GB} // 30) * $KB_PER_GB;
my $max_age  = ($ENV{CARGO_BUILD_GC_MAX_DAYS} // 3) * 86400;
my $dry_run  = ($ENV{CARGO_BUILD_GC_DRY_RUN}  // '') eq '1';

my $cargo_home = $ENV{CARGO_HOME} // "$ENV{HOME}/.cargo";
my $root       = realpath("$cargo_home/build");
exit 0 unless defined $root && -d $root;
my $trash = "$root/.gc-trash";
my $seen  = "$root/.gc-last-busy";

sub logline { print strftime('%F %T', localtime), " @_\n" }

# Leftovers from a run that stopped between rename and delete.
remove_tree($trash) unless $dry_run;

my @commands = `ps -ax -o command=`;
die "ps failed\n" if $?;

sub runs_from {
    my ($ws) = @_;
    return scalar grep { index($_, "$ws/") >= 0 } @commands;
}

# Returns open, exclusively locked handles for every lock of the folder, or
# undef when Cargo holds one of them.
sub lock_all {
    my ($ws) = @_;
    my @held;
    for my $path (map { bsd_glob($_) } "$ws/*/.cargo-lock", "$ws/*/*/.cargo-lock") {
        next if -l $path;
        open(my $fh, '<', $path) or next;
        flock($fh, LOCK_EX | LOCK_NB) or return undef;
        push @held, $fh;
    }
    return \@held;
}

sub busy {
    my ($ws) = @_;
    my $locks = lock_all($ws);
    return 1 unless $locks;
    return runs_from($ws) ? 1 : 0;
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
    my $out = `du -sk "$_[0]"`;
    return ($out =~ /^(\d+)/) ? $1 : 0;
}

sub free_kb {
    my @lines = `df -k "$root"`;
    my @cols  = split ' ', ($lines[1] // '');
    return $cols[3] // 0;
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
        my $id = "$shard$name";
        if (busy($ws)) {
            make_path($seen);
            if (open(my $mark, '>>', "$seen/$id")) { close($mark) }
            utime(undef, undef, "$seen/$id");
        }
        push @workspaces, { ws => $ws, id => $id, used => last_used($ws, $id), kb => size_kb($ws) };
    }
    closedir($dh);
}
closedir($rd);

my $total = 0;
$total += $_->{kb} for @workspaces;
my $deleted = 0;

for my $w (sort { $a->{used} <=> $b->{used} } @workspaces) {
    my $age = $now - $w->{used};
    my $why =
        $age > $max_age                         ? 'not used for ' . int($age / 86400) . ' days'
      : $total > $cap_kb                        ? 'over cap'
      : (free_kb() < $floor_kb && $age > 3600)  ? 'below free-disk floor'
      :                                           undef;
    next unless $why;

    my $locks = lock_all($w->{ws});
    if (!$locks || runs_from($w->{ws})) {
        logline("skip $w->{ws} ($why): in use");
        next;
    }
    logline("delete $w->{ws} ($why, " . int($w->{kb} / 1024) . ' MB)');
    $total -= $w->{kb};
    next if $dry_run;

    make_path($trash);
    my $dest = "$trash/$w->{id}-$$";
    rename($w->{ws}, $dest) or do { logline("rename failed for $w->{ws}: $!"); next };
    undef $locks;
    remove_tree($dest);
    $deleted++;
    unlink("$seen/$w->{id}");
    rmdir(($w->{ws} =~ m{^(.*)/[^/]+$})[0]);
}

logline(sprintf('kept %d folders, %.1f GB; %.1f GB free', @workspaces - $deleted, $total / $KB_PER_GB, free_kb() / $KB_PER_GB));
