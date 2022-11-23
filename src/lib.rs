extern crate core;

pub mod lib {
    use crate::lib::conn::Connection;
    use crate::lib::frame::Frame;
    use bytes::Bytes;
    use dashmap::DashMap;
    use mini_redis::Command;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;

    pub mod cmd;
    pub mod conn;
    pub mod frame;
    pub mod parse;

    ///大多数函数返回的错误。
    /// 在编写真正的应用程序时，可能需要考虑专门的错误处理箱或将错误类型定义为原因的枚举。但是，对于我们的示例，使用装箱的 std::error::Error 就足够了。
    /// 出于性能原因，在任何热路径中都应避免装箱。例如，在 parse 中，定义了一个自定义错误枚举。这是因为当在套接字上接收到部分帧时，在正常执行期间会遇到并处理错误。 std::error::Error 是为 parse::Error 实现的，它允许将其转换为 Box<dyn std::error::Error>。
    pub type Error = Box<dyn std::error::Error + Send + Sync>;

    ///项目用Result
    pub type Result<T> = std::result::Result<T, Error>;

    type DB = Arc<DashMap<String, Bytes>>;

    pub async fn run() {
        let listener = TcpListener::bind("127.0.0.1:6378").await.unwrap();
        let db: DB = Arc::new(DashMap::new());
        db.insert("ping".to_string(), "pong".into());
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let arc_db = db.clone();
            println!("get some");
            tokio::spawn(async move { process(stream, arc_db).await });
        }
    }

    async fn process(socket: TcpStream, db: DB) {
        let mut conn = Connection::new(socket);
        if let Some(frame) = conn.read_frame().await.unwrap() {
            if let Ok(cmd) = Command::from_frame(frame) {
                let resp = match cmd {
                    Command::Get(cmd) => match db.get(cmd.key()) {
                        None => Frame::Null,
                        Some(value) => Frame::Bulk(value.value().clone()),
                    },
                    _ => {
                        unimplemented!()
                    }
                };
                conn.write_frame(resp).await.unwrap();
            }
        }
    }
}
