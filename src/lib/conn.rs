use crate::lib;
use crate::lib::frame::Frame;
use bytes::{Buf, BytesMut};
use std::io::Cursor;
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;

#[derive(Debug)]
pub(crate) struct Connection {
    //对于TCP连接的缓冲写入
    stream: BufWriter<TcpStream>,
    //作为一个空的缓冲区
    buffer: BytesMut,
}

const KB: usize = 1024;
//结束符
const CRLF: &[u8; 2] = b"\r\n";

impl Connection {
    ///创建一个新的连接
    pub fn new(socket: TcpStream) -> Connection {
        Connection {
            stream: BufWriter::new(socket),
            buffer: BytesMut::with_capacity(4 * KB),
        }
    }

    ///从字节流中读取数据，并解析出Frame
    fn parse_frame(&mut self) -> lib::Result<Option<Frame>> {
        use lib::frame::FrameError::Incomplete;
        let mut buf = Cursor::new(&self.buffer[..]);
        match Frame::check(&mut buf) {
            Ok(_) => {
                let len = buf.position() as usize;
                buf.set_position(0);
                //从字节流中将Frame解析出来
                let frame = Frame::parse(&mut buf)?;
                self.buffer.advance(len);
                Ok(Some(frame))
            }
            Err(Incomplete) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    ///从字节流中尝试读取frame
    pub async fn read_frame(&mut self) -> lib::Result<Option<Frame>> {
        loop {
            //如果可以解析出一个帧则返回解析出来的frame，直接返回
            //parse_frame中会自动消耗buffer中的数据
            //使用loop的原因是可能目前获取的命令不全，与前一个命令发生了粘包
            // 导致缓存内部命令残缺所以需要多次读取
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }
            //从self的stream流中将数据读入buffer中，
            if self.stream.read_buf(&mut self.buffer).await? == 0 {
                return if self.buffer.is_empty() {
                    Ok(None)
                } else {
                    Err("链接强制中断".into())
                };
            }
        }
    }

    ///redis的传输协议
    ///
    ///1、对于简单字符串，回复的第一个字节是“+”，后续直接加字符串内容，一般来说比较短
    ///
    /// 2、对于错误，回复的第一个字节是“ - ”，与简单字符串差不多
    ///
    /// 3、对于整数，回复的第一个字节是“：”，后续直接加数字
    ///
    /// 4、对于批量字符串，回复的第一个字节是“$”，
    ///
    /// 5、对于数组，回复的第一个字节是“*”，格式为“${长度} {内容}”，长度为-1时代表为空
    ///
    /// 进行解析
    pub async fn write_frame(&mut self, frame: Frame) -> io::Result<()> {
        match frame {
            Frame::Array(target_vec) => {
                self.stream.write_u8(b'*').await?;
                self.write_decimal(target_vec.len() as u64).await?;
                for cur in target_vec {
                    self.write_value(&cur).await?;
                }
            }
            _ => self.write_value(&frame).await?,
        }
        self.stream.flush().await
    }

    //将除了数组以外的对象写入stream
    async fn write_value(&mut self, frame: &Frame) -> io::Result<()> {
        match frame {
            Frame::Simple(val) => {
                self.stream.write_u8(b'+').await?;
                self.stream.write_all(val.as_bytes()).await?;
                self.stream.write_all(CRLF).await?;
            }
            Frame::Error(val) => {
                self.stream.write_u8(b'-').await?;
                self.stream.write_all(val.as_bytes()).await?;
                self.stream.write_all(CRLF).await?;
            }
            Frame::Integer(val) => {
                self.stream.write_u8(b':').await?;
                self.write_decimal(*val).await?;
            }
            Frame::Bulk(val) => {
                let len = val.len();
                self.stream.write_u8(b'$').await?;
                self.write_decimal(len as u64).await?;
                self.stream.write_all(val).await?;
                self.stream.write_all(CRLF).await?;
            }
            Frame::Null => {
                self.stream.write_all(b"$-1").await?;
                self.stream.write_all(CRLF).await?
            }
            Frame::Array(_) => unreachable!(),
        }
        Ok(())
    }

    //写入多位数字
    async fn write_decimal(&mut self, val: u64) -> io::Result<()> {
        use std::io::Write;

        let mut buf = [0u8; 20];
        let mut buf_cur = Cursor::new(&mut buf[..]);
        //需要通过cursor进行写入
        write!(&mut buf_cur, "{}", val)?;
        let len = buf_cur.position() as usize;
        self.stream.write_all(&buf_cur.get_ref()[0..len]).await?;
        self.stream.write_all(CRLF).await?;
        Ok(())
    }
}
