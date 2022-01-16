use std::sync::MutexGuard;
use std::sync::atomic::Ordering::Relaxed;
use crate::channel::Channel;
use crate::tcp::Client;
use crate::{strings, message, size, global, ChannelExec, ChannelShell};
use crate::error::{SshError, SshErrorKind, SshResult};
use crate::kex::Kex;
use crate::packet::{Data, Packet};
use crate::util;


pub struct Session;


impl Session {
    pub fn connect(&mut self) -> Result<(), SshError> {
        // 版本协商
        // 获取服务端版本
        self.receive_version()?;
        // 版本验证
        let config = util::config()?;
        config.version.validation()?;
        util::unlock(config);
        // 发送客户端版本
        self.send_version()?;

        // 密钥协商
        let mut kex = Kex::new()?;
        kex.send_algorithm()?;
        kex.receive_algorithm()?;

        let config = util::config()?;
        let (dh, sign) = config.algorithm.matching_algorithm()?;
        kex.dh = dh;
        kex.signature = sign;

        kex.h.set_v_c(config.version.client_version.as_str());
        kex.h.set_v_s(config.version.server_version.as_str());

        util::unlock(config);

        kex.send_qc()?;
        kex.verify_signature()?;
        kex.new_keys()?;

        self.initiate_authentication()?;
        self.authentication()
    }

    pub fn set_nonblocking(&mut self, nonblocking: bool) -> SshResult<()> {
        let client = util::client()?;
        if let Err(e) = client.stream.set_nonblocking(nonblocking) {
            return Err(SshError::from(e))
        }
        Ok(())
    }

    pub fn set_user_and_password(&mut self, user: String, password: String) -> SshResult<()> {
        let mut config = util::config()?;
        config.user.username = user;
        config.user.password = password;
        Ok(())
    }

    pub fn close(self) -> SshResult<()> {
        let mut client = util::client()?;
        client.close()
    }

    pub fn open_channel(&mut self) -> SshResult<Channel> {
        let client_channel = global::CLIENT_CHANNEL.load(Relaxed);
        self.ssh_open_channel(client_channel)?;
        global::CLIENT_CHANNEL.fetch_add(1, Relaxed);
        Ok(Channel {
            kex: Kex::new()?,
            server_channel: 0,
            client_channel,
            remote_close: false,
            local_close: false
        })
    }

    pub fn open_exec(&mut self) -> SshResult<ChannelExec> {
        let channel = self.open_channel()?;
        channel.open_exec()
    }

    pub fn open_shell(&mut self) -> SshResult<ChannelShell> {
        let channel = self.open_channel()?;
        channel.open_shell()
    }

    fn ssh_open_channel(&mut self, client_channel: u32) -> SshResult<()> {
        let mut data = Data::new();
        data.put_u8(message::SSH_MSG_CHANNEL_OPEN)
            .put_str(strings::SESSION)
            .put_u32(client_channel)
            .put_u32(size::LOCAL_WINDOW_SIZE)
            .put_u32(size::BUF_SIZE as u32);
        let mut packet = Packet::from(data);
        packet.build();
        let mut client = util::client()?;
        client.write(packet.as_slice())
    }

    fn initiate_authentication(&mut self) -> SshResult<()> {
        let mut data = Data::new();
        data.put_u8(message::SSH_MSG_SERVICE_REQUEST)
            .put_str(strings::SSH_USERAUTH);
        let mut packet = Packet::from(data);
        packet.build();
        let mut client = util::client()?;
        client.write(packet.as_slice())
    }

    fn authentication(&mut self) -> SshResult<()> {
        let mut client = util::client()?;
        loop {
            let results = client.read()?;
            for result in results {
                if result.is_empty() { continue }
                let message_code = result[5];
                match message_code {
                    message::SSH_MSG_SERVICE_ACCEPT => {
                        // 开始密码验证 TODO 目前只支持密码验证
                        password_authentication(&mut client)?;
                    }
                    message::SSH_MSG_USERAUTH_FAILURE => {
                        log::error!("user auth failure");
                        return Err(SshError::from(SshErrorKind::PasswordError))
                    },
                    message::SSH_MSG_USERAUTH_SUCCESS => {
                        log::info!("user auth success");
                        return Ok(())
                    },
                    message::SSH_MSG_GLOBAL_REQUEST => {
                        let mut data = Data::new();
                        data.put_u8(message::SSH_MSG_REQUEST_FAILURE);
                        let mut packet = Packet::from(data);
                        packet.build();
                        client.write(packet.as_slice())?
                    }
                    _ => {}
                }
            }
        }
    }

    fn send_version(&mut self) -> SshResult<()> {
        let mut client = util::client()?;
        let config = util::config()?;
        client.write_version(format!("{}\r\n", config.version.client_version).as_bytes())?;
        log::info!("client version => {}", config.version.client_version);
        Ok(())
    }

    fn receive_version(&mut self) -> SshResult<()> {
        let mut client = util::client()?;
        let vec = client.read_version();
        let from_utf8 = util::from_utf8(vec)?;
        let sv = from_utf8.trim();
        log::info!("server version => {}", sv);
        let mut config = util::config()?;
        config.version.server_version = sv.to_string();
        Ok(())
    }
}


fn password_authentication(client: &mut MutexGuard<'static, Client>) -> SshResult<()> {
    let config = util::config()?;
    if config.user.username.is_empty() {
        return Err(SshError::from(SshErrorKind::UserNullError))
    }
    if config.user.password.is_empty() {
        return Err(SshError::from(SshErrorKind::PasswordNullError))
    }

    let mut data = Data::new();
    data.put_u8(message::SSH_MSG_USERAUTH_REQUEST)
        .put_str(config.user.username.as_str())
        .put_str(strings::SSH_CONNECTION)
        .put_str(strings::PASSWORD)
        .put_u8(false as u8)
        .put_str(config.user.password.as_str());
    let mut packet = Packet::from(data);
    packet.build();
    client.write(packet.as_slice())
}

