use std::{env, path::PathBuf};

pub fn socket_path() -> PathBuf {
	let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
		.map(PathBuf::from)
		.unwrap_or_else(env::temp_dir);
	runtime_dir.join(format!("nurapods-{}.sock", unsafe { libc::getuid() }))
}
