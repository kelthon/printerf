//! Um `cupsd` privado, em espaço de usuário, para testes end-to-end.
//!
//! * Não toca no CUPS do sistema: diretórios, socket e fila vivem num tempdir.
//! * O binário é **copiado** para o tempdir: o perfil AppArmor do Ubuntu/Pop!_OS
//!   (`/etc/apparmor.d/usr.sbin.cupsd`) é associado ao caminho `/usr/sbin/cupsd` e
//!   impediria escrever fora de `/var/spool/cups`.
//! * A fila de teste é `raw` apontando para `file:///dev/null`: nenhum papel é gasto.
//!
//! Sem `cupsd`/`lpadmin` instalados o teste é pulado, a menos que
//! `PRINTERF_E2E_REQUIRED=1` (usado no CI) — aí falha alto.

#![allow(dead_code, clippy::unwrap_used)] // helper de teste: falhar alto é o desejado

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const QUEUE: &str = "PrinterF_E2E";

pub struct PrivateCups {
    pub dir: tempfile::TempDir,
    child: Child,
}

fn which(bin: &str, fallbacks: &[&str]) -> Option<PathBuf> {
    fallbacks
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
        .or_else(|| std::env::var_os("PATH")?.to_str()?.split(':').map(|d| Path::new(d).join(bin)).find(|p| p.exists()))
}

impl PrivateCups {
    /// Sobe o cupsd e cria a fila [`QUEUE`]. `None` = dependências ausentes (teste pulado).
    pub fn start() -> Option<Self> {
        let required = std::env::var("PRINTERF_E2E_REQUIRED").is_ok_and(|v| v == "1");
        let (Some(cupsd), Some(_)) = (which("cupsd", &["/usr/sbin/cupsd"]), which("lpadmin", &["/usr/sbin/lpadmin"]))
        else {
            assert!(!required, "PRINTERF_E2E_REQUIRED=1 mas cupsd/lpadmin não estão instalados");
            eprintln!("⚠ cupsd/lpadmin ausentes — teste e2e pulado (apt install cups)");
            return None;
        };

        // sun_path tem 108 bytes: prefira XDG_RUNTIME_DIR (/run/user/N), curto.
        let base = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let dir = tempfile::Builder::new().prefix("pf-e2e.").tempdir_in(base).expect("tempdir");
        let d = dir.path();
        for sub in ["spool", "tmp", "cache", "state", "log"] {
            std::fs::create_dir_all(d.join(sub)).unwrap();
        }
        let group = String::from_utf8(Command::new("id").arg("-gn").output().unwrap().stdout).unwrap();
        let group = group.trim();
        let dd = d.display();

        std::fs::write(
            d.join("cups-files.conf"),
            format!(
                "ServerRoot {dd}\nServerBin /usr/lib/cups\nDataDir /usr/share/cups\nRequestRoot {dd}/spool\n\
                 TempDir {dd}/tmp\nCacheDir {dd}/cache\nStateDir {dd}/state\nAccessLog {dd}/log/access_log\n\
                 ErrorLog {dd}/log/error_log\nPageLog {dd}/log/page_log\nFileDevice Yes\n\
                 Group {group}\nSystemGroup {group}\n"
            ),
        )
        .unwrap();
        std::fs::write(
            d.join("cupsd.conf"),
            format!(
                "Listen {dd}/cups.sock\nLogLevel debug\nBrowsing No\nDefaultAuthType None\nWebInterface No\n\
                 PreserveJobFiles Yes\nPreserveJobHistory Yes\n\
                 <Location />\n  Order Allow,Deny\n  Allow all\n</Location>\n\
                 <Policy default>\n  <Limit All>\n    Order Deny,Allow\n  </Limit>\n</Policy>\n"
            ),
        )
        .unwrap();

        let local_cupsd = d.join("cupsd");
        std::fs::copy(&cupsd, &local_cupsd).unwrap();
        let child = Command::new(&local_cupsd)
            .arg("-f")
            .arg("-c")
            .arg(d.join("cupsd.conf"))
            .arg("-s")
            .arg(d.join("cups-files.conf"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("iniciando cupsd privado");
        let me = Self { dir, child };

        let deadline = Instant::now() + Duration::from_secs(10);
        while !me.socket().exists() {
            assert!(Instant::now() < deadline, "cupsd não abriu o socket; log:\n{}", me.error_log());
            std::thread::sleep(Duration::from_millis(50));
        }

        // libcups lê CUPS_SERVER na primeira chamada de cada thread.
        // SAFETY: feito antes de qualquer outra thread do teste usar libcups.
        unsafe {
            std::env::set_var("CUPS_SERVER", me.socket());
            // Amostras (página de teste) também ficam dentro do tempdir.
            std::env::set_var("XDG_CACHE_HOME", me.dir.path().join("xdg-cache"));
        }

        me.admin("lpadmin", &["-p", QUEUE, "-E", "-v", "file:///dev/null", "-m", "raw"]);
        Some(me)
    }

    pub fn socket(&self) -> PathBuf {
        self.dir.path().join("cups.sock")
    }

    pub fn error_log(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("log/error_log")).unwrap_or_default()
    }

    /// Roda `lpadmin`/`cupsenable`/`cupsdisable` contra o cupsd privado.
    pub fn admin(&self, cmd: &str, args: &[&str]) {
        let out = Command::new(cmd).arg("-h").arg(self.socket()).args(args).output().expect(cmd);
        assert!(out.status.success(), "{cmd} {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// Arquivos de documento preservados pelo cupsd (`PreserveJobFiles Yes`).
    pub fn spooled_document(&self, job_id: i32) -> PathBuf {
        self.dir.path().join(format!("spool/d{job_id:05}-001"))
    }
}

impl Drop for PrivateCups {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Espera até `cond` ser verdadeira (ou falha após `secs`).
pub fn wait_until(secs: u64, what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while !cond() {
        assert!(Instant::now() < deadline, "timeout esperando: {what}");
        std::thread::sleep(Duration::from_millis(100));
    }
}
