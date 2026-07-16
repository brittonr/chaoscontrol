use std::env;
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_long};

const EXPECTED_ARG_COUNT: usize = 2;
const SYS_DELETE_MODULE_X86_64: c_long = 176;
const DELETE_MODULE_FLAGS_NONE: c_int = 0;
const MAX_MODULE_NAME_BYTES: usize = 128;

unsafe extern "C" {
    fn syscall(number: c_long, ...) -> c_long;
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if let Err(error) = run(&args) {
        eprintln!("kernel-bundle-delete-module: {error}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<(), String> {
    if args.len() != EXPECTED_ARG_COUNT {
        return Err("usage: kernel-bundle-delete-module <module-name>".to_string());
    }
    let module_name = validate_module_name(&args[1])?;
    delete_module_exact(&module_name)
}

fn validate_module_name(value: &str) -> Result<CString, String> {
    if value.is_empty() {
        return Err("module name must not be empty".to_string());
    }
    if value.len() > MAX_MODULE_NAME_BYTES {
        return Err(format!(
            "module name exceeds {MAX_MODULE_NAME_BYTES} bytes: {}",
            value.len()
        ));
    }
    CString::new(value).map_err(|_| "module name contains an interior NUL byte".to_string())
}

fn delete_module_exact(module_name: &CString) -> Result<(), String> {
    let result = unsafe {
        syscall(
            SYS_DELETE_MODULE_X86_64,
            module_name.as_ptr() as *const c_char,
            DELETE_MODULE_FLAGS_NONE,
        )
    };
    if result == 0 {
        return Ok(());
    }
    Err(std::io::Error::last_os_error().to_string())
}
