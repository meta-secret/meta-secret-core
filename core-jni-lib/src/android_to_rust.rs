#[cfg(target_os = "android")]
#[allow(non_snake_case)]
pub mod android {

    extern crate jni;
    use self::jni::JNIEnv;
    use self::jni::objects::{JClass, JString};
    use self::jni::sys::jstring;

    #[no_mangle]
    pub extern "system" fn Java_com_example_metasecret_android_RustWrapper_greet<'local>(
        mut env: JNIEnv<'local>,
        _class: JClass<'local>,
        input: JString<'local>,
    ) -> jstring {
        let input_str: String = env.get_string(&input).expect("Couldn't get java string!").into();
        let output = env.new_string(&input_str).expect("Couldn't create Java String!");
        output.into_raw()
    }
}