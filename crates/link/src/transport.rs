//! The private endpoint (docs/specs/mcp.md §5.2).
//!
//! - **Unix:** a socket at `<run>/<pid>.sock`, in a folder that must be the
//!   user's own with mode 0700, created with mode 0600. Each connection's
//!   peer must run as the same user.
//! - **Windows:** a named pipe with a random name, created as the first
//!   instance of that name, refusing remote clients, with an access list
//!   that grants access to the pipe's owner only.

use std::io::{self, Read, Write};
use std::path::Path;

use interprocess::ConnectWaitMode;
use interprocess::local_socket::traits::Stream as _;
use interprocess::local_socket::{
    ConnectOptions, GenericFilePath, Listener, ListenerNonblockingMode, ListenerOptions, RecvHalf,
    SendHalf, Stream, ToFsName,
};

/// How long a connect may wait for a busy endpoint.
const CONNECT_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// The app's listening endpoint.
pub struct Server {
    listener: Listener,
    /// What clients connect to, as the session file gives it.
    pub endpoint: String,
    #[cfg(unix)]
    socket: std::path::PathBuf,
    #[cfg(unix)]
    uid: u32,
}

/// One connection, on either side.
pub struct Conn {
    stream: Stream,
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::io;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};

    /// Makes `dir` this user's private folder, or says why it can't be.
    /// Returns the user's id, learned from a file created in it.
    pub fn private_dir(dir: &Path, pid: u32) -> io::Result<u32> {
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        let probe = dir.join(format!(".probe-{pid}"));
        fs::write(&probe, b"")?;
        let uid = fs::metadata(&probe)?.uid();
        fs::remove_file(&probe)?;
        let m = fs::symlink_metadata(dir)?;
        if !m.is_dir() || m.uid() != uid || m.mode() & 0o077 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "{} is not a folder that only you can use, so no agent link is offered",
                    dir.display()
                ),
            ));
        }
        Ok(uid)
    }

    /// The socket path, moved to a short private folder when the run
    /// folder's path is too long for a socket address.
    pub fn socket_path(run: &Path, pid: u32) -> io::Result<(PathBuf, u32)> {
        let uid = private_dir(run, pid)?;
        let sock = run.join(format!("{pid}.sock"));
        if sock.as_os_str().len() <= 100 {
            return Ok((sock, uid));
        }
        let short = std::env::temp_dir().join(format!("coldopen-{uid}"));
        private_dir(&short, pid)?;
        Ok((short.join(format!("{pid}.sock")), uid))
    }
}

impl Server {
    /// Starts listening for this process.
    pub fn start(run: &Path, pid: u32) -> io::Result<Server> {
        #[cfg(unix)]
        {
            use interprocess::os::unix::local_socket::ListenerOptionsExt;
            let (socket, uid) = unix::socket_path(run, pid)?;
            // A socket left by a crashed process with this pid.
            let _ = std::fs::remove_file(&socket);
            let name = || socket.as_path().to_fs_name::<GenericFilePath>();
            // Mode 0600 where the OS lets a socket have one before it is
            // bound (Linux). macOS does not; there the 0700 folder, which no
            // one else can enter, and the peer check protect it.
            let listener = match ListenerOptions::new()
                .name(name()?)
                .mode(0o600)
                .create_sync()
            {
                Err(e) if e.kind() == io::ErrorKind::Unsupported => {
                    let _ = std::fs::remove_file(&socket);
                    ListenerOptions::new().name(name()?).create_sync()?
                }
                other => other?,
            };
            Ok(Server {
                listener,
                endpoint: socket.display().to_string(),
                socket,
                uid,
            })
        }
        #[cfg(windows)]
        {
            use interprocess::os::windows::local_socket::ListenerOptionsExt;
            use interprocess::os::windows::security_descriptor::SecurityDescriptor;
            let _ = (run, pid);
            let endpoint = format!(r"\\.\pipe\coldopen-{}", crate::random_hex(16)?);
            // Protected DACL with one entry: full access for the owner.
            let sddl = widestring::U16CString::from_str("D:P(A;;GA;;;OW)")
                .map_err(|e| io::Error::other(e.to_string()))?;
            let sd = SecurityDescriptor::deserialize(&sddl)?;
            let name = endpoint.as_str().to_fs_name::<GenericFilePath>()?;
            let listener = ListenerOptions::new()
                .name(name)
                .security_descriptor(sd)
                .create_sync()?;
            Ok(Server { listener, endpoint })
        }
    }

    /// Waits for the next connection.
    pub fn accept(&self) -> io::Result<Conn> {
        use interprocess::local_socket::traits::Listener as _;
        self.listener.accept().map(|stream| Conn { stream })
    }

    /// Makes [`Server::accept`] return `WouldBlock` instead of waiting.
    pub fn set_nonblocking(&self, on: bool) -> io::Result<()> {
        use interprocess::local_socket::traits::Listener as _;
        self.listener.set_nonblocking(if on {
            ListenerNonblockingMode::Accept
        } else {
            ListenerNonblockingMode::Neither
        })
    }

    /// Whether the connection's peer runs as this user. On Windows the
    /// pipe's access list has already decided.
    pub fn same_user(&self, conn: &Conn) -> bool {
        #[cfg(unix)]
        {
            use interprocess::local_socket::traits::StreamCommon as _;
            conn.stream.peer_creds().ok().and_then(|c| c.euid()) == Some(self.uid)
        }
        #[cfg(not(unix))]
        {
            let _ = conn;
            true
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(&self.socket);
        }
    }
}

impl Conn {
    /// Connects, waiting at most two seconds for a busy endpoint: a pipe
    /// whose every instance is in use would otherwise wait for ever.
    pub fn connect(endpoint: &str) -> io::Result<Conn> {
        let name = endpoint.to_fs_name::<GenericFilePath>()?;
        ConnectOptions::new()
            .name(name)
            .wait_mode(ConnectWaitMode::Timeout(CONNECT_WAIT))
            .connect_sync()
            .map(|stream| Conn { stream })
    }

    pub fn set_nonblocking(&self, on: bool) -> io::Result<()> {
        self.stream.set_nonblocking(on)
    }

    /// Separate halves, for a reading thread and a writing one.
    pub fn split(self) -> (RecvHalf, SendHalf) {
        self.stream.split()
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LineReader;

    #[test]
    fn a_line_each_way() {
        let dir = tempfile::tempdir().unwrap();
        let server = Server::start(&dir.path().join("run"), std::process::id()).unwrap();
        let endpoint = server.endpoint.clone();
        let client = std::thread::spawn(move || {
            let mut c = Conn::connect(&endpoint).unwrap();
            c.write_all(b"ping\n").unwrap();
            let mut r = LineReader::new(c);
            r.read_line(100).unwrap()
        });
        let conn = server.accept().unwrap();
        assert!(server.same_user(&conn));
        let (recv, mut send) = conn.split();
        let mut r = LineReader::new(recv);
        assert_eq!(r.read_line(100).unwrap().as_deref(), Some("ping"));
        send.write_all(b"pong\n").unwrap();
        assert_eq!(client.join().unwrap().as_deref(), Some("pong"));
    }

    #[test]
    fn nobody_listens_at_a_made_up_endpoint() {
        #[cfg(windows)]
        let e = r"\\.\pipe\coldopen-0000000000000000nobody";
        #[cfg(unix)]
        let e = "/nonexistent/coldopen.sock";
        assert!(Conn::connect(e).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_shared_run_folder_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let run = dir.path().join("run");
        std::fs::create_dir(&run).unwrap();
        // A folder we cannot make private: a symlink to elsewhere.
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&run, &link).unwrap();
        std::fs::set_permissions(&run, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(Server::start(&link, 1).is_err());
    }
}
