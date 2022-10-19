use std::path::Path;
use crate::{Session, SshError, SshResult};
use crate::algorithm::hash::HashType;
use crate::constant::{ssh_msg_code, ssh_str};
use crate::data::Data;
use crate::h::H;
use crate::key_pair::{KeyPair, KeyPairType};
use crate::user_info::UserInfo;

impl Session {

 fn get_user_info(&self) -> SshResult<&UserInfo> {
        if self.user_info.is_none() {
            return Err(SshError::from("user info is none."))
        }
        Ok(self.user_info.as_ref().unwrap())
    }

    pub fn auth_user_info(&mut self, user_info: UserInfo) {
        self.user_info = Some(user_info);
    }

    pub fn set_user_and_password<U: ToString, P: ToString>(&mut self, username: U, password: P) {
        let user_info = UserInfo::from_password(username.to_string(), password.to_string());
        self.auth_user_info(user_info);
    }

    pub fn set_user_and_key_pair<U: ToString, K: ToString>(&mut self, username: U, key_str: K, key_type: KeyPairType) -> SshResult<()> {
        let pair = KeyPair::from_str(key_str.to_string().as_str(), key_type)?;
        let user_info = UserInfo::from_key_pair(username, pair);
        self.auth_user_info(user_info);
        Ok(())
    }

    pub fn set_user_and_key_pair_path
    <U: ToString, P: AsRef<Path>>
    (&mut self,
     username: U,
     key_path: P,
     key_type: KeyPairType)
        -> SshResult<()>
    {
        let pair = KeyPair::from_path(key_path, key_type)?;
        let user_info = UserInfo::from_key_pair(username.to_string(), pair);
        self.auth_user_info(user_info);
        Ok(())
    }

    pub(crate) fn password_authentication(&mut self) -> SshResult<()> {
        log::info!("password authentication.");
        let user_info = self.get_user_info()?;
        let mut data = Data::new();
        data.put_u8(ssh_msg_code::SSH_MSG_USERAUTH_REQUEST)
            .put_str(user_info.username.as_str())
            .put_str(ssh_str::SSH_CONNECTION)
            .put_str(ssh_str::PASSWORD)
            .put_u8(false as u8)
            .put_str(user_info.password.as_str());
        self.client.as_mut().unwrap().write(data)
    }

    pub(crate) fn public_key_authentication(&mut self) -> SshResult<()> {
        log::info!("public key authentication.");
        let user_info = self.get_user_info()?;
        let mut data = Data::new();
        data.put_u8(ssh_msg_code::SSH_MSG_USERAUTH_REQUEST)
            .put_str(user_info.username.as_str())
            .put_str(ssh_str::SSH_CONNECTION)
            .put_str(ssh_str::PUBLIC_KEY)
            .put_u8(false as u8)
            .put_str(user_info.key_pair.key_type.as_str())
            .put_u8s(user_info.key_pair.blob.as_slice());
        self.client.as_mut().unwrap().write(data)
    }

    pub(crate) fn public_key_signature(&mut self, ht: HashType, h: H) -> SshResult<()> {
        let user_info = self.get_user_info()?;
        let mut data = Data::new();
        data.put_u8(ssh_msg_code::SSH_MSG_USERAUTH_REQUEST)
            .put_str(user_info.username.as_str())
            .put_str(ssh_str::SSH_CONNECTION)
            .put_str(ssh_str::PUBLIC_KEY)
            .put_u8(true as u8)
            .put_str(user_info.key_pair.key_type.as_str())
            .put_u8s(user_info.key_pair.blob.as_slice());
        let signature = user_info.key_pair.signature(data.as_slice(), h, ht);
        data.put_u8s(&signature);
        self.client.as_mut().unwrap().write(data)
    }
}
