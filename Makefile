all:
	@cargo build --release

debug:
	@cargo build

clean:
	@cargo clean

musl: test_musl_env
	@RUSTFLAGS="-L $$MUSL_STATIC_LIB_DIR" LIBCAP_LIB_TYPE=static LIBSECCOMP_LIB_TYPE=static cargo build --release --target x86_64-unknown-linux-musl

musl_debug: test_musl_env
	@RUSTFLAGS="-L $$MUSL_STATIC_LIB_DIR" LIBCAP_LIB_TYPE=static LIBSECCOMP_LIB_TYPE=static cargo build --target x86_64-unknown-linux-musl

test_musl_env:
	@if [[ ! "$$MUSL_STATIC_LIB_DIR" ]]; then\
		echo 'Set MUSL_STATIC_LIB_DIR to a directory containing libcap.a and libseccomp.a statically linked to musl';\
		exit 1;\
	fi
