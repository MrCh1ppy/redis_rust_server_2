use crate::lib;
use bytes::{Buf, Bytes};
use core::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::io::Cursor;
use std::num::TryFromIntError;
use std::string::FromUtf8Error;

/// 帧
/// 用于和读取的字节进行中继
#[derive(Clone, Debug)]
pub enum Frame {
    ///简单字符串
    ///
    ///对于简单字符串，回复的第一个字节是“+”，后续直接加字符串内容，一般来说比较短
    Simple(String),
    ///错误
    ///
    /// 对于错误，回复的第一个字节是“ - ”，与简单字符串差不多
    Error(String),
    ///整型
    ///
    /// 对于整数，回复的第一个字节是“：”，后续直接加数字
    Integer(u64),
    ///大容量字节
    ///
    /// 对于大容量字符串，回复的第一个字节是“$”，
    Bulk(Bytes),
    /// 空
    Null,
    ///数组
    ///
    /// 对于数组，回复的第一个字节是“*”，格式为“${长度} {内容}”，长度为-1时代表为空
    Array(Vec<Frame>),
}

#[derive(Debug)]
pub enum FrameError {
    ///字节不全，无法解析成Frame
    Incomplete,
    ///其他错误
    Other(lib::Error),
}

impl Frame {
    ///创建一个数组
    pub(crate) fn array() -> Frame {
        Frame::Array(vec![])
    }

    pub(crate) fn push_bulk(&mut self, bytes: Bytes) {
        match self {
            Frame::Array(vec) => vec.push(Frame::Bulk(bytes)),
            _ => panic!("not an bulk array"),
        }
    }

    pub(crate) fn push_int(&mut self, value: u64) {
        match self {
            Frame::Array(vec) => vec.push(Frame::Integer(value)),
            _ => panic!(),
        }
    }

    ///查看是否可以将流中的数据转化为帧
    pub fn check(src: &mut Cursor<&[u8]>) -> Result<(), FrameError> {
        match get_u8(src)? {
            b'+' => {
                get_line(src)?;
                Ok(())
            }
            b'-' => {
                get_line(src)?;
                Ok(())
            }
            b':' => {
                get_decimal(src)?;
                Ok(())
            }
            b'$' => {
                if peek_u8(src)? == b'-' {
                    skip(src, 4_usize)
                } else {
                    let len: usize = get_decimal(src)?.try_into()?;
                    skip(src, len + 2)
                }
            }
            b'*' => {
                let len: usize = get_decimal(src)?.try_into()?;
                for _ in 0..len {
                    Frame::check(src)?;
                }
                Ok(())
            }
            actual => Err(format!("校验发生错误，错误内容：{}", actual).into()),
        }
    }

    pub fn parse(src: &mut Cursor<&[u8]>) -> Result<Frame, FrameError> {
        match get_u8(src)? {
            b'+' => {
                let text = get_line(src)?.to_vec();
                let text = String::from_utf8(text)?;
                Ok(Frame::Simple(text))
            }
            b'-' => {
                let line = get_line(src)?.to_vec();
                let text = String::from_utf8(line)?;
                Ok(Frame::Simple(text))
            }
            b':' => {
                let num = get_decimal(src)?;
                Ok(Frame::Integer(num))
            }
            b'$' => {
                let flag = peek_u8(src)?;
                if flag == b'-' {
                    let line = get_line(src)?;
                    if line == b"-1" {
                        return Err("非法协议，大容量字符串长度为-1以外负数".into());
                    }
                    Ok(Frame::Null)
                } else {
                    let size: usize = get_decimal(src)?.try_into()?;
                    if src.remaining() < size + 2 {
                        return Err(FrameError::Incomplete);
                    }
                    let data = Bytes::copy_from_slice(&src.chunk()[..size]);
                    skip(src, size + 2)?;
                    Ok(Frame::Bulk(data))
                }
            }
            b'*' => {
                let size = get_decimal(src)?;
                let mut vec = Vec::with_capacity(size as usize);
                for _ in 0..size {
                    let frame = Frame::parse(src)?;
                    vec.push(frame);
                }
                todo!()
            }
            _ => Err("解析发生错误".into()),
        }
    }
}

///查看下一个u8的数值
fn peek_u8(src: &mut Cursor<&[u8]>) -> Result<u8, FrameError> {
    if !src.has_remaining() {
        return Err(FrameError::Incomplete);
    }

    Ok(src.chunk()[0])
}

///获得流中下一个u8的值
fn get_u8(src: &mut Cursor<&[u8]>) -> Result<u8, FrameError> {
    if !src.has_remaining() {
        return Err(FrameError::Incomplete);
    }
    Ok(src.get_u8())
}

///跳过range个字节
fn skip(src: &mut Cursor<&[u8]>, range: usize) -> Result<(), FrameError> {
    if !src.remaining() < range {
        return Err(FrameError::Incomplete);
    }
    src.advance(range);
    Ok(())
}

///获取一整行
fn get_line<'a>(src: &mut Cursor<&'a [u8]>) -> Result<&'a [u8], FrameError> {
    let start = src.position() as usize;
    let end = src.get_ref().len() as usize;
    for i in start..end {
        if src.get_ref()[i] == b'\r' && src.get_ref()[i + 1] == b'\n' {
            src.set_position((i + 2) as u64);
            return Ok(&src.get_ref()[start..i]);
        }
    }
    Err(FrameError::Incomplete)
}

/// 解析并获取下一个u64
fn get_decimal(src: &mut Cursor<&[u8]>) -> Result<u64, FrameError> {
    use atoi::atoi;

    let line = get_line(src)?;

    atoi::<u64>(line).ok_or_else(|| "从流中获取u64失败".into())
}

impl Display for Frame {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use core::str;

        match self {
            Frame::Simple(text) => Display::fmt(text, f),

            Frame::Error(msg) => write!(f, "错误error:{}", msg),

            Frame::Integer(value) => write!(f, "{}", value),

            Frame::Bulk(value) => match str::from_utf8(value) {
                Ok(text) => Display::fmt(text, f),
                Err(_) => write!(f, "{:?}", value),
            },

            Frame::Null => Display::fmt("(nil)", f),

            Frame::Array(vec) => {
                for cur in vec.iter().skip(1) {
                    write!(f, " ")?;
                    Display::fmt(cur, f)?;
                }
                Ok(())
            }
        }
    }
}

impl PartialEq<&str> for Frame {
    fn eq(&self, other: &&str) -> bool {
        match self {
            Frame::Simple(s) => s.eq(other),
            Frame::Bulk(s) => s.eq(other),
            _ => false,
        }
    }
}

impl From<String> for FrameError {
    fn from(src: String) -> FrameError {
        FrameError::Other(src.into())
    }
}

impl From<&str> for FrameError {
    fn from(src: &str) -> FrameError {
        FrameError::Other(src.into())
    }
}

impl From<TryFromIntError> for FrameError {
    fn from(err: TryFromIntError) -> Self {
        FrameError::Other(err.into())
    }
}

impl From<FromUtf8Error> for FrameError {
    fn from(err: FromUtf8Error) -> Self {
        FrameError::Other(err.into())
    }
}

impl Display for FrameError {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::Incomplete => Display::fmt("流过早关闭", fmt),
            FrameError::Other(err) => write!(fmt, "错误error:{}", err),
        }
    }
}

impl std::error::Error for FrameError {}
