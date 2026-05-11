use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result};
use argon2::Argon2;
use rand::RngCore;
use std::fs;
use std::path::PathBuf;
use stopvibe_common::{BlockSession, STATE_DIR, STATE_FILE};

const SALT: &[u8] = b"stopvibe-machine-bound-salt-2026";
const NONCE_LEN: usize = 12;

pub struct StateManager {
    state_path: PathBuf,
    key: [u8; 32],
}

impl StateManager {
    pub fn new() -> Result<Self> {
        let base = std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
        let dir = PathBuf::from(base).join(STATE_DIR);
        fs::create_dir_all(&dir).context("Failed to create state directory")?;
        let state_path = dir.join(STATE_FILE);

        let machine_id = get_machine_id();
        let key = derive_key(&machine_id);

        Ok(Self { state_path, key })
    }

    pub fn save_session(&self, session: &BlockSession) -> Result<()> {
        let plaintext = serde_json::to_vec(session)?;
        let cipher = Aes256Gcm::new_from_slice(&self.key).unwrap();

        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_ref())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Format: nonce || ciphertext
        let mut output = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);

        fs::write(&self.state_path, &output).context("Failed to write state file")?;
        Ok(())
    }

    pub fn load_session(&self) -> Result<Option<BlockSession>> {
        if !self.state_path.exists() {
            return Ok(None);
        }

        let data = fs::read(&self.state_path).context("Failed to read state file")?;
        if data.len() < NONCE_LEN {
            return Ok(None);
        }

        let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
        let cipher = Aes256Gcm::new_from_slice(&self.key).unwrap();
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

        let session: BlockSession = serde_json::from_slice(&plaintext)?;
        Ok(Some(session))
    }

    pub fn clear_session(&self) -> Result<()> {
        if self.state_path.exists() {
            fs::remove_file(&self.state_path)?;
        }
        Ok(())
    }

    pub fn state_path(&self) -> &PathBuf {
        &self.state_path
    }
}

fn get_machine_id() -> Vec<u8> {
    // Use MachineGuid from registry as machine-specific identifier
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output();

    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            // Extract the GUID value
            if let Some(line) = text.lines().find(|l| l.contains("MachineGuid")) {
                line.split_whitespace()
                    .last()
                    .unwrap_or("fallback-id")
                    .as_bytes()
                    .to_vec()
            } else {
                b"fallback-machine-id".to_vec()
            }
        }
        Err(_) => b"fallback-machine-id".to_vec(),
    }
}

fn derive_key(machine_id: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(machine_id, SALT, &mut key)
        .expect("Key derivation failed");
    key
}
