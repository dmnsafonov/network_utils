all:
	@cargo build --release

debug:
	@cargo build

clean:
	@cargo clean

musl_rsndpproxy: test_musl_env
	@RUSTFLAGS="-L $$MUSL_STATIC_LIB_DIR" LIBCAP_LIB_TYPE=static cargo build --package rsndpproxy --release --target x86_64-unknown-linux-musl

musl_debug_rsndpproxy: test_musl_env
	@RUSTFLAGS="-L $$MUSL_STATIC_LIB_DIR" LIBCAP_LIB_TYPE=static cargo build ---package rsndpproxy --target x86_64-unknown-linux-musl

test_musl_env:
	@if [[ ! "$$MUSL_STATIC_LIB_DIR" ]]; then\
		echo 'Set MUSL_STATIC_LIB_DIR to a directory containing libcap.a linked to musl';\
		exit 1;\
	fi
