// Helper struct that we'll use to give strings to C.
type SizeT = usize;

#[repr(C)]
pub struct RustByteSlice {
    pub bytes: *const u8,
    pub len: SizeT,
}

impl<'a> From<&'a str> for RustByteSlice {
    fn from(s: &'a str) -> Self {
        RustByteSlice{
            bytes: s.as_ptr(),
            len: s.len() as SizeT,
        }
    }
}