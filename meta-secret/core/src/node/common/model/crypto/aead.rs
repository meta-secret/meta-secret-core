use age::armor::{ArmoredWriter, Format};
use std::io::Write;

use crate::crypto::encoding::base64::Base64Text;
use crate::crypto::keys::TransportSk;
use crate::node::common::model::crypto::channel::CommunicationChannel;
use anyhow::{bail, Result};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AeadCipherText {
    pub msg: Base64Text,
    pub channel: CommunicationChannel,
}

impl AeadCipherText {
    /// Decrypt this secret message using the secret key
    pub fn decrypt(&self, sk: &TransportSk) -> Result<AeadPlainText> {
        if !self.channel.contains(&sk.pk()?) {
            bail!("Invalid recipient")
        }

        let decrypted_vec = {
            let encrypted = Vec::try_from(&self.msg)?;
            age::decrypt(&sk.as_age()?, encrypted.as_slice())?
        };

        let plain_text = AeadPlainText {
            msg: Base64Text::from(decrypted_vec),
            channel: self.channel.clone(),
        };

        Ok(plain_text)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AeadPlainText {
    pub msg: Base64Text,
    pub channel: CommunicationChannel,
}

impl AeadPlainText {
    pub fn encrypt(&self) -> Result<AeadCipherText> {
        let encryptor = {
            let recipients = self.channel.recipients()?;
            age::Encryptor::with_recipients(recipients.iter().map(|r| r.as_ref() as _))?
        };

        let mut ciphertext = vec![];

        let armored_writer = ArmoredWriter::wrap_output(&mut ciphertext, Format::AsciiArmor)?;
        let mut writer = encryptor.wrap_output(armored_writer)?;

        let plaintext = String::try_from(&self.msg)?;
        writer.write_all(plaintext.as_bytes())?;
        writer.finish()?.finish()?;

        let msg = Base64Text::from(ciphertext);

        let cipher_text = AeadCipherText {
            msg,
            channel: self.channel.clone(),
        };

        Ok(cipher_text)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EncryptedMessage {
    /// There is only one type of encrypted message for now, which is encrypted share of a secret,
    /// and that particular type of message has a device link,
    /// and it used to figure out which vault the message belongs to
    CipherShare { share: AeadCipherText },
}

impl EncryptedMessage {
    pub fn cipher_text(&self) -> &AeadCipherText {
        match self {
            EncryptedMessage::CipherShare { share, .. } => share,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::key_pair::KeyPair;
    use crate::crypto::keys::KeyManager;
    use crate::node::common::model::crypto::aead::AeadCipherText;
    use crate::secret::shared_secret::PlainText;

    #[test]
    fn encryption_test() -> anyhow::Result<()> {
        let password = PlainText::from("2bee~");
        let alice_km = KeyManager::generate();
        let bob_km = KeyManager::generate();

        let _: AeadCipherText = alice_km
            .transport
            .encrypt_string(password.clone(), &bob_km.transport.pk())?;

        Ok(())
    }
}
