use crate::lib::cmd::get::Get;

mod get;

#[derive(Debug)]
pub enum Command{
    Get(Get)
}



