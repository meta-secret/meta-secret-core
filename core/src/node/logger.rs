pub trait MetaLogger: Send + Sync + 'static {
    fn debug(&self, msg: &str);
    fn info(&self, msg: &str);
    fn warn(&self, msg: &str);
    fn error(&self, msg: &str);

    fn id(&self) -> LoggerId;
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoggerId {
    Client,
    Server,
    Vd1,
    Vd2,
    Test,
}

pub struct DefaultMetaLogger {
    pub id: LoggerId,
}

impl MetaLogger for DefaultMetaLogger {
    fn debug(&self, msg: &str) {
        println!("{:?}", msg);
    }
    fn info(&self, msg: &str) {
        println!("{:?}", msg);
    }

    fn warn(&self, msg: &str) {
        println!("{:?}", msg);
    }

    fn error(&self, msg: &str) {
        println!("{:?}", msg);
    }

    fn id(&self) -> LoggerId {
        self.id.clone()
    }
}

impl DefaultMetaLogger {
    pub fn new(id: LoggerId) -> Self {
        Self { id }
    }
}
