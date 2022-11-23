use crate::lib;
use crate::lib::frame::Frame;
use std::vec::IntoIter;

///用于解析命令的实用程序
///
///命令表示为数组帧。框架中的每个条目都是一个“令牌”。 Parse使用数组框架进行初始化，
/// 并提供类似游标的 API。每个命令结构都包含一个parse_frame方法，
/// 该方法使用Parse来提取其字段。
///
/// 命令作为一个数组来存储命令的各个部分，组合起来形成拼接成一个完整的命令
#[derive(Debug)]
pub struct Parse {
    //一个帧迭代器
    part: IntoIter<Frame>,
}

#[derive(Debug)]
pub(crate) enum ParseError {
    EndOfStream,
    Other(lib::Error),
}

impl Parse {
    ///创建一个新的解析器
    ///
    /// 每个命令使用一个单独的解析器
    pub(crate) fn new(frame: Frame) -> Result<Parse, ParseError> {
        let array = match frame {
            Frame::Array(array) => array,
            _ => return Err("协议错误,命令必须为一个帧数组".into()),
        };
        Ok(Parse {
            part: array.into_iter(),
        })
    }

    ///获取命令中的下一帧
    fn next(&mut self) -> Result<Frame, ParseError> {
        self.part.next().ok_or(ParseError::EndOfStream)
    }

    pub(crate) fn next_string(&mut self) -> Result<String, ParseError> {
        match self.next()? {
            Frame::Simple(text) => Ok(text),
            Frame::Bulk(data) => {
                let src = data.to_vec();
                String::from_utf8(src).map_err(|_| "解析的比特无法转化为字符串".into())
            }
            frame => {
                Err(format!("解析错误，预计获取的帧为简单字符串或大容量比特，实际获取的为:{}",frame).into())
            }
        }
    }
}

impl From<&str> for ParseError {
    fn from(text: &str) -> Self {
        ParseError::Other(text.to_string().into())
    }
}

impl From<String> for ParseError {
    fn from(text: String) -> Self {
        ParseError::Other(text.into())
    }
}


