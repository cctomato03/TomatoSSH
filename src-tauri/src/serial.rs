use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serialport::{ClearBuffer, DataBits, FlowControl, Parity, SerialPort, StopBits};
use tauri::{AppHandle, Emitter};

use crate::crypto::b64_encode;
use crate::error::{Error, Result};
use crate::models::{SerialPortInfo, SerialProfile};

/// 读线程每次最多读这么多字节
const READ_BUF: usize = 4096;
/// 读超时：既能及时把数据推给前端，又能让线程在关闭时尽快退出
const READ_TIMEOUT_MS: u64 = 30;

struct SerialSession {
    writer: Box<dyn SerialPort>,
    closed: Arc<AtomicBool>,
}

#[derive(Default, Clone)]
pub struct SerialManager {
    sessions: Arc<Mutex<HashMap<String, SerialSession>>>,
}

fn parse_data_bits(bits: u8) -> Result<DataBits> {
    match bits {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        other => Err(Error::msg(format!("不支持的数据位：{other}（可选 5/6/7/8）"))),
    }
}

fn parse_parity(p: &str) -> Result<Parity> {
    match p.to_ascii_lowercase().as_str() {
        "none" | "n" | "" => Ok(Parity::None),
        "odd" | "o" => Ok(Parity::Odd),
        "even" | "e" => Ok(Parity::Even),
        other => Err(Error::msg(format!("不支持的校验位：{other}（none/odd/even）"))),
    }
}

fn parse_stop_bits(bits: u8) -> Result<StopBits> {
    match bits {
        1 => Ok(StopBits::One),
        2 => Ok(StopBits::Two),
        other => Err(Error::msg(format!("不支持的停止位：{other}（1/2）"))),
    }
}

fn parse_flow_control(f: &str) -> Result<FlowControl> {
    match f.to_ascii_lowercase().as_str() {
        "none" | "" => Ok(FlowControl::None),
        "software" | "xonxoff" => Ok(FlowControl::Software),
        "hardware" | "rtscts" => Ok(FlowControl::Hardware),
        other => Err(Error::msg(format!(
            "不支持的流控：{other}（none/software/hardware）"
        ))),
    }
}

/// 枚举当前系统可用的串口
pub fn list_ports() -> Result<Vec<SerialPortInfo>> {
    let ports = serialport::available_ports()
        .map_err(|e| Error::msg(format!("枚举串口失败：{e}")))?;
    let mut out: Vec<SerialPortInfo> = ports
        .into_iter()
        .map(|p| {
            let (kind, detail) = match p.port_type {
                serialport::SerialPortType::UsbPort(info) => (
                    "usb".to_string(),
                    Some(
                        format!(
                            "VID:{:04X} PID:{:04X} {}",
                            info.vid,
                            info.pid,
                            info.product.unwrap_or_default()
                        )
                        .trim()
                        .to_string(),
                    ),
                ),
                serialport::SerialPortType::BluetoothPort => ("bluetooth".to_string(), None),
                serialport::SerialPortType::PciPort => ("pci".to_string(), None),
                serialport::SerialPortType::Unknown => ("unknown".to_string(), None),
            };
            SerialPortInfo {
                name: p.port_name,
                kind,
                detail: detail.filter(|d| !d.is_empty()),
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 按配置打开串口句柄（拆出来便于单测错误路径）
pub fn open_port(profile: &SerialProfile) -> Result<Box<dyn SerialPort>> {
    let port_name = profile.port_name.trim();
    if port_name.is_empty() {
        return Err(Error::msg("串口设备为空，请先在配置里选择端口"));
    }
    serialport::new(port_name, profile.baud_rate)
        .data_bits(parse_data_bits(profile.data_bits)?)
        .parity(parse_parity(&profile.parity)?)
        .stop_bits(parse_stop_bits(profile.stop_bits)?)
        .flow_control(parse_flow_control(&profile.flow_control)?)
        .timeout(Duration::from_millis(READ_TIMEOUT_MS))
        .open()
        .map_err(|e| Error::msg(format!("打开串口失败（{port_name}）：{e}")))
}

impl SerialManager {
    /// 打开串口并开始把收到的数据推给前端
    pub fn open(
        &self,
        app: AppHandle,
        session_id: String,
        profile: &SerialProfile,
    ) -> Result<String> {
        let port_name = profile.port_name.trim().to_string();
        if self.has(&session_id) {
            self.close(&session_id);
        }

        let mut port = open_port(profile)?;

        // 清掉上次残留的输入缓冲，避免一打开就刷出一堆旧数据
        let _ = port.clear(ClearBuffer::Input);

        let mut reader = port
            .try_clone()
            .map_err(|e| Error::msg(format!("复制串口句柄失败（{port_name}）：{e}")))?;

        let closed = Arc::new(AtomicBool::new(false));
        self.sessions.lock()?.insert(
            session_id.clone(),
            SerialSession {
                writer: port,
                closed: closed.clone(),
            },
        );

        let sid = session_id.clone();
        let app_data = app.clone();
        let app_exit = app.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; READ_BUF];
            loop {
                if closed.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => continue,
                    Ok(n) => {
                        let _ = app_data.emit(&format!("term-data:{sid}"), b64_encode(&buf[..n]));
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        let _ = app_exit.emit(
                            &format!("term-exit:{sid}"),
                            serde_json::json!({ "code": null, "message": format!("串口读取中断：{e}") }),
                        );
                        return;
                    }
                }
            }
            let _ = app_exit.emit(
                &format!("term-exit:{sid}"),
                serde_json::json!({ "code": null, "message": "串口已关闭" }),
            );
        });

        Ok(format!(
            "\x1b[38;5;244m—— 已打开串口 {port_name}（{} {} {}{}，流控 {}）——\x1b[0m",
            profile.baud_rate,
            profile.data_bits,
            profile.parity.to_ascii_uppercase(),
            profile.stop_bits,
            profile.flow_control
        ))
    }

    pub fn write(&self, session_id: &str, data: &[u8]) -> Result<()> {
        let mut guard = self.sessions.lock()?;
        let session = guard
            .get_mut(session_id)
            .ok_or_else(|| Error::msg("串口会话不存在或已关闭"))?;
        session
            .writer
            .write_all(data)
            .map_err(|e| Error::msg(format!("写入串口失败：{e}")))?;
        session.writer.flush().ok();
        Ok(())
    }

    pub fn close(&self, session_id: &str) {
        if let Ok(mut guard) = self.sessions.lock() {
            if let Some(session) = guard.remove(session_id) {
                session.closed.store(true, Ordering::Relaxed);
                // writer 随 session drop 关闭句柄；读线程会在下次超时后退出
            }
        }
    }

    pub fn has(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .map(|g| g.contains_key(session_id))
            .unwrap_or(false)
    }

    pub fn count(&self) -> usize {
        self.sessions.lock().map(|g| g.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_serial_enums() {
        assert!(matches!(parse_data_bits(8).unwrap(), DataBits::Eight));
        assert!(parse_data_bits(4).is_err());
        assert!(matches!(parse_parity("none").unwrap(), Parity::None));
        assert!(matches!(parse_parity("EVEN").unwrap(), Parity::Even));
        assert!(parse_parity("mark").is_err());
        assert!(matches!(parse_stop_bits(1).unwrap(), StopBits::One));
        assert!(parse_stop_bits(3).is_err());
        assert!(matches!(
            parse_flow_control("hardware").unwrap(),
            FlowControl::Hardware
        ));
        assert!(matches!(
            parse_flow_control("").unwrap(),
            FlowControl::None
        ));
        assert!(parse_flow_control("dtr").is_err());
    }

    #[test]
    fn lists_ports_without_panicking() {
        // 没插设备时返回空列表也是正确行为，这里只要求不报错
        match list_ports() {
            Ok(ports) => println!("检测到 {} 个串口: {:?}", ports.len(), ports),
            Err(e) => println!("枚举串口返回错误（部分平台无串口时正常）: {e}"),
        }
    }

    #[test]
    fn open_invalid_port_gives_chinese_error() {
        let profile = SerialProfile {
            port_name: "COM_DOES_NOT_EXIST_999".into(),
            baud_rate: 115_200,
            ..Default::default()
        };
        let err = open_port(&profile).err().expect("不存在的串口必须报错");
        let msg = err.to_string();
        assert!(msg.contains("打开串口失败"), "错误信息应可读：{msg}");
        assert!(msg.contains("COM_DOES_NOT_EXIST_999"), "错误信息应包含端口名：{msg}");

        // 空端口名在打开前就被拦下
        let empty = SerialProfile::default();
        assert!(open_port(&empty).is_err());

        // 参数非法也在打开前报错（不会真的去碰硬件）
        let bad_bits = SerialProfile {
            port_name: "COM1".into(),
            data_bits: 9,
            ..Default::default()
        };
        assert!(open_port(&bad_bits).is_err(), "9 位数据位必须被拒绝");
        assert_eq!(SerialManager::default().count(), 0);
    }
}
