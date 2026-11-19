import os
from pathlib import Path
import subprocess
import tempfile


class GitRepoFixture:
    """Temporary git repo with one commit; mix into a unittest.TestCase.

    Kept out of the TestCase hierarchy so that test modules that share it do not
    inherit, and so re-run, each other's tests.
    """

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.previous = Path.cwd()
        os.chdir(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('config', 'user.name', 'Test')
        self.write('code.txt', 'before')
        self.commit()
        self.base = self.git('rev-parse', 'HEAD')
        self.cfg = {'rules': [{'id': 'contract', 'paths': ['contracts/**'], 'tier': 'stop'}], 'dependency_files': ['package.json'], 'version_files': ['package.json'], 'patch_bots': ['bot']}

    def tearDown(self):
        os.chdir(self.previous)
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(['git', *args], text=True).strip()

    def write(self, path, text):
        Path(path).parent.mkdir(parents=True, exist_ok=True)
        Path(path).write_text(text)

    def commit(self):
        self.git('add', '.')
        self.git('commit', '-qm', 'fixture')
