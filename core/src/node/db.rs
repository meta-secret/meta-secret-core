use std::fmt::Error;
use crate::models::{UserSecurityBox, UserSignature};

pub trait SecurityBoxRepo {
    fn save(security_box: UserSecurityBox);
    fn get() -> Result<UserSecurityBox, Error>;
}

pub trait UserSigRepo {
    fn save(security_box: UserSignature);
    fn get() -> Result<UserSignature, Error>;
}

pub trait UserRepo : SecurityBoxRepo + UserSigRepo {

}