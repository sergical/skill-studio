use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub fn run(command: &mut Command) -> Vec<u8> {
    let output = command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{command:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

pub struct GitPack {
    pub work: PathBuf,
    pub url: String,
    address: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl GitPack {
    pub fn new(root: &Path) -> Self {
        let work = root.join("upstream");
        let repos = root.join("repos");
        std::fs::create_dir_all(repos.join("owner")).unwrap();
        run(Command::new("git")
            .args(["init", "--bare"])
            .arg(repos.join("owner/pack.git")));
        run(Command::new("git").args(["init", "-b", "main"]).arg(&work));
        for (key, value) in [
            ("user.name", "Contract"),
            ("user.email", "contract@example.invalid"),
        ] {
            run(Command::new("git")
                .current_dir(&work)
                .args(["config", key, value]));
        }
        run(Command::new("git")
            .current_dir(&work)
            .args(["remote", "add", "origin"])
            .arg(repos.join("owner/pack.git")));
        run(Command::new("git")
            .args(["--git-dir"])
            .arg(repos.join("owner/pack.git"))
            .args(["symbolic-ref", "HEAD", "refs/heads/main"]));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let server = thread::spawn(move || {
            for stream in listener.incoming() {
                if stopped.load(Ordering::SeqCst) {
                    break;
                }
                serve(stream.unwrap(), &repos);
            }
        });
        let pack = Self {
            work,
            url: format!("http://{address}/owner/pack.git"),
            address,
            stop,
            server: Some(server),
        };
        for name in ["a", "b", "c"] {
            pack.skill(name, "v1");
        }
        pack.publish("v1");
        pack
    }

    pub fn skill(&self, name: &str, version: &str) {
        let dir = self.work.join("skills").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), skill_body(name, version)).unwrap();
    }

    pub fn publish(&self, tag: &str) {
        for args in [
            vec!["add", "."],
            vec!["commit", "-m", tag],
            vec!["tag", tag],
            vec!["push", "origin", "main", "--tags"],
        ] {
            run(Command::new("git").current_dir(&self.work).args(args));
        }
    }
}

pub fn skill_body(name: &str, version: &str) -> String {
    format!("---\nname: {name}\ndescription: Test skill {name}\n---\n{version}\n")
}

impl Drop for GitPack {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        self.server.take().unwrap().join().unwrap();
    }
}

fn serve(mut stream: TcpStream, repos: &Path) {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let request: Vec<_> = line.split_whitespace().collect();
    let method = request[0].to_string();
    let (path, query) = request[1].split_once('?').unwrap_or((request[1], ""));
    let mut backend = Command::new("git");
    backend
        .arg("http-backend")
        .env("GIT_PROJECT_ROOT", repos)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("REQUEST_METHOD", method)
        .env("PATH_INFO", path)
        .env("QUERY_STRING", query)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header == "\r\n" {
            break;
        }
        let (key, value) = header.split_once(':').unwrap();
        let value = value.trim();
        let env = match key.to_ascii_lowercase().as_str() {
            "content-type" => "CONTENT_TYPE",
            "content-length" => {
                length = value.parse().unwrap();
                "CONTENT_LENGTH"
            }
            "git-protocol" => "HTTP_GIT_PROTOCOL",
            "content-encoding" => "HTTP_CONTENT_ENCODING",
            _ => continue,
        };
        backend.env(env, value);
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    let mut child = backend.spawn().unwrap();
    child.stdin.take().unwrap().write_all(&body).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let split = output
        .stdout
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap();
    let headers = String::from_utf8_lossy(&output.stdout[..split]);
    let mut status = "200 OK";
    let mut forwarded = Vec::new();
    for header in headers.lines() {
        if let Some(value) = header.strip_prefix("Status: ") {
            status = value;
        } else {
            forwarded.push(header);
        }
    }
    let body = &output.stdout[split + 4..];
    write!(
        stream,
        "HTTP/1.1 {status}\r\n{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        forwarded.join("\r\n"),
        body.len()
    )
    .unwrap();
    stream.write_all(body).unwrap();
}
