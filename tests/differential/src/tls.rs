//! Phase 03.1 differential-harness PKI module. Builds a self-signed CA + leafs
//! `a.example.com`, `b.example.com`, `envoy-rust.test` in a per-fixture
//! `TempDir`. Both upstream-Envoy (containerized) and envoy-rust (host
//! subprocess) reference the same PEMs via `render_yaml` substitution; the
//! envoy-side paths point inside `/etc/envoy-rust-tls/` (mounted via
//! `with_copy_to_container` in `upstream::start`), while the subject-side
//! paths point at the host tmpdir.
//!
//! 03.1 only uses `leaf_a` + `ca`; `leaf_b` and `server` PEMs are generated
//! anyway (cheap; avoids extending TlsTestPki later) so 03.2 can layer on the
//! SNI fixtures with no harness changes.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rcgen::{CertificateParams, DnType, IsCa, KeyPair, KeyUsagePurpose};
use tempfile::TempDir;

/// Container-side path prefix; PEMs land here via testcontainers'
/// `with_copy_to_container`. SPEC §6 signpost 7 / parent-SPEC §6 signpost 12.
pub const ENVOY_SIDE_DIR: &str = "/etc/envoy-rust-tls";

pub struct TlsTestPki {
    pub ca_pem_path: PathBuf,
    pub leaf_a_cert: PathBuf,
    pub leaf_a_key: PathBuf,
    pub leaf_b_cert: PathBuf,
    pub leaf_b_key: PathBuf,
    pub server_cert: PathBuf,
    pub server_key: PathBuf,
    _dir: TempDir,
}

impl TlsTestPki {
    /// Generate a CA + three leafs (`a.example.com`, `b.example.com`,
    /// `envoy-rust.test`) signed by the CA. PEMs are written into a per-call
    /// `TempDir` whose `Drop` removes everything.
    pub fn generate() -> Result<Self> {
        let dir = tempfile::tempdir().context("creating PKI tmpdir")?;
        let (ca_cert, ca_kp) = build_ca()?;

        let (leaf_a_cert, leaf_a_kp) = build_leaf(&ca_cert, &ca_kp, "a.example.com")?;
        let (leaf_b_cert, leaf_b_kp) = build_leaf(&ca_cert, &ca_kp, "b.example.com")?;
        let (srv_cert, srv_kp) = build_leaf(&ca_cert, &ca_kp, "envoy-rust.test")?;

        let ca_pem = ca_cert.pem();
        let ca_pem_path = dir.path().join("ca.pem");
        std::fs::write(&ca_pem_path, &ca_pem).context("write ca.pem")?;

        let leaf_a_cert_path = dir.path().join("leaf-a-cert.pem");
        let leaf_a_key_path = dir.path().join("leaf-a-key.pem");
        std::fs::write(&leaf_a_cert_path, leaf_a_cert.pem()).context("write leaf-a cert")?;
        std::fs::write(&leaf_a_key_path, leaf_a_kp.serialize_pem()).context("write leaf-a key")?;

        let leaf_b_cert_path = dir.path().join("leaf-b-cert.pem");
        let leaf_b_key_path = dir.path().join("leaf-b-key.pem");
        std::fs::write(&leaf_b_cert_path, leaf_b_cert.pem()).context("write leaf-b cert")?;
        std::fs::write(&leaf_b_key_path, leaf_b_kp.serialize_pem()).context("write leaf-b key")?;

        let srv_cert_path = dir.path().join("server-cert.pem");
        let srv_key_path = dir.path().join("server-key.pem");
        std::fs::write(&srv_cert_path, srv_cert.pem()).context("write server cert")?;
        std::fs::write(&srv_key_path, srv_kp.serialize_pem()).context("write server key")?;

        Ok(Self {
            ca_pem_path,
            leaf_a_cert: leaf_a_cert_path,
            leaf_a_key: leaf_a_key_path,
            leaf_b_cert: leaf_b_cert_path,
            leaf_b_key: leaf_b_key_path,
            server_cert: srv_cert_path,
            server_key: srv_key_path,
            _dir: dir,
        })
    }

    /// Path map for the envoy.yaml side: keys are the substitution tokens,
    /// values are container-mounted paths under `/etc/envoy-rust-tls/`.
    pub fn envoy_side_paths(&self) -> HashMap<&'static str, String> {
        let mut m = HashMap::new();
        m.insert("CA_PATH", format!("{ENVOY_SIDE_DIR}/ca.pem"));
        m.insert(
            "LEAF_A_CERT_PATH",
            format!("{ENVOY_SIDE_DIR}/leaf-a-cert.pem"),
        );
        m.insert(
            "LEAF_A_KEY_PATH",
            format!("{ENVOY_SIDE_DIR}/leaf-a-key.pem"),
        );
        // 03.2 will reference these via the SNI fixture; harmless to expose now.
        m.insert(
            "LEAF_B_CERT_PATH",
            format!("{ENVOY_SIDE_DIR}/leaf-b-cert.pem"),
        );
        m.insert(
            "LEAF_B_KEY_PATH",
            format!("{ENVOY_SIDE_DIR}/leaf-b-key.pem"),
        );
        m.insert(
            "SERVER_CERT_PATH",
            format!("{ENVOY_SIDE_DIR}/server-cert.pem"),
        );
        m.insert(
            "SERVER_KEY_PATH",
            format!("{ENVOY_SIDE_DIR}/server-key.pem"),
        );
        m
    }

    /// Path map for the envoy-rust.yaml side: keys are the same substitution
    /// tokens, values are the actual host tmpdir paths.
    pub fn subject_side_paths(&self) -> HashMap<&'static str, String> {
        let mut m = HashMap::new();
        m.insert("CA_PATH", self.ca_pem_path.to_string_lossy().into_owned());
        m.insert(
            "LEAF_A_CERT_PATH",
            self.leaf_a_cert.to_string_lossy().into_owned(),
        );
        m.insert(
            "LEAF_A_KEY_PATH",
            self.leaf_a_key.to_string_lossy().into_owned(),
        );
        m.insert(
            "LEAF_B_CERT_PATH",
            self.leaf_b_cert.to_string_lossy().into_owned(),
        );
        m.insert(
            "LEAF_B_KEY_PATH",
            self.leaf_b_key.to_string_lossy().into_owned(),
        );
        m.insert(
            "SERVER_CERT_PATH",
            self.server_cert.to_string_lossy().into_owned(),
        );
        m.insert(
            "SERVER_KEY_PATH",
            self.server_key.to_string_lossy().into_owned(),
        );
        m
    }

    /// Files to mount into the upstream container via
    /// `with_copy_to_container`. Returns `(host_path, container_path)` pairs.
    /// SPEC §6 signpost 7.
    pub fn container_mounts(&self) -> Vec<(PathBuf, String)> {
        vec![
            (self.ca_pem_path.clone(), format!("{ENVOY_SIDE_DIR}/ca.pem")),
            (
                self.leaf_a_cert.clone(),
                format!("{ENVOY_SIDE_DIR}/leaf-a-cert.pem"),
            ),
            (
                self.leaf_a_key.clone(),
                format!("{ENVOY_SIDE_DIR}/leaf-a-key.pem"),
            ),
            (
                self.leaf_b_cert.clone(),
                format!("{ENVOY_SIDE_DIR}/leaf-b-cert.pem"),
            ),
            (
                self.leaf_b_key.clone(),
                format!("{ENVOY_SIDE_DIR}/leaf-b-key.pem"),
            ),
            (
                self.server_cert.clone(),
                format!("{ENVOY_SIDE_DIR}/server-cert.pem"),
            ),
            (
                self.server_key.clone(),
                format!("{ENVOY_SIDE_DIR}/server-key.pem"),
            ),
        ]
    }
}

fn build_ca() -> Result<(rcgen::Certificate, KeyPair)> {
    let mut params =
        CertificateParams::new(vec!["envoy-rust-test-ca".into()]).context("ca params")?;
    params
        .distinguished_name
        .push(DnType::CommonName, "envoy-rust-test-ca");
    params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let kp = KeyPair::generate().context("ca kp")?;
    let cert = params.self_signed(&kp).context("ca self-sign")?;
    Ok((cert, kp))
}

fn build_leaf(
    ca_cert: &rcgen::Certificate,
    ca_kp: &KeyPair,
    san_dns: &str,
) -> Result<(rcgen::Certificate, KeyPair)> {
    let mut params = CertificateParams::new(vec![san_dns.into()]).context("leaf params")?;
    params.distinguished_name.push(DnType::CommonName, san_dns);
    let kp = KeyPair::generate().context("leaf kp")?;
    let cert = params
        .signed_by(&kp, ca_cert, ca_kp)
        .with_context(|| format!("signing leaf for {san_dns}"))?;
    Ok((cert, kp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_test_pki_generates_valid_chain() {
        let pki = TlsTestPki::generate().expect("generate");
        for (label, path) in &[
            ("ca", &pki.ca_pem_path),
            ("leaf_a_cert", &pki.leaf_a_cert),
            ("leaf_a_key", &pki.leaf_a_key),
            ("leaf_b_cert", &pki.leaf_b_cert),
            ("leaf_b_key", &pki.leaf_b_key),
            ("server_cert", &pki.server_cert),
            ("server_key", &pki.server_key),
        ] {
            assert!(path.exists(), "{label} missing at {}", path.display());
            let content = std::fs::read(path).expect("read");
            // `rustls-pemfile::certs` returns ≥1 entry on a cert PEM and zero
            // on a key PEM; the inverse holds for `private_key`. Use `certs`
            // for cert-shaped paths and `private_key` for key-shaped paths.
            if label.ends_with("cert") || *label == "ca" {
                let mut s = content.as_slice();
                let collected: Vec<_> = rustls_pemfile::certs(&mut s).collect();
                assert!(
                    !collected.is_empty(),
                    "{label} contains no certificate at {}",
                    path.display()
                );
            } else {
                // keys
                let mut s = content.as_slice();
                let key = rustls_pemfile::private_key(&mut s).expect("parse key");
                assert!(
                    key.is_some(),
                    "{label} contains no private key at {}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn tls_test_pki_drop_removes_tmpdir() {
        let pki = TlsTestPki::generate().expect("generate");
        let captured = pki.ca_pem_path.clone();
        assert!(captured.exists());
        drop(pki);
        assert!(
            !captured.exists(),
            "ca path still exists after Drop: {}",
            captured.display()
        );
    }

    #[test]
    fn envoy_side_paths_returns_container_paths() {
        let pki = TlsTestPki::generate().expect("generate");
        let paths = pki.envoy_side_paths();
        assert_eq!(paths.get("CA_PATH").unwrap(), "/etc/envoy-rust-tls/ca.pem");
        assert_eq!(
            paths.get("LEAF_A_CERT_PATH").unwrap(),
            "/etc/envoy-rust-tls/leaf-a-cert.pem"
        );
        assert_eq!(
            paths.get("LEAF_A_KEY_PATH").unwrap(),
            "/etc/envoy-rust-tls/leaf-a-key.pem"
        );
    }

    #[test]
    fn subject_side_paths_returns_host_tmpdir_paths() {
        let pki = TlsTestPki::generate().expect("generate");
        let paths = pki.subject_side_paths();
        let ca = paths.get("CA_PATH").unwrap();
        assert!(
            ca.contains(std::env::temp_dir().to_string_lossy().as_ref())
                || ca.starts_with("/tmp/")
                || ca.starts_with("/var/folders/"),
            "subject-side CA path should be under tmp: {ca}",
        );
        // The actual file must exist.
        assert!(std::path::Path::new(ca).exists());
    }
}
