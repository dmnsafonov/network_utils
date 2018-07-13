all:
	@cargo build --release

debug:
	@cargo build

clean:
	@cargo clean

static_musl: test_musl_dir
	@LIBCAP_LIB_PATH="$$MUSL_STATIC_LIB_DIR" LIBSECCOMP_LIB_PATH="$$MUSL_STATIC_LIB_DIR" \
        LIBCAP_LIB_TYPE=static LIBSECCOMP_LIB_TYPE=static \
        cargo build --release --target x86_64-unknown-linux-musl

static_musl_debug: test_musl_dir
	@LIBCAP_LIB_PATH="$$MUSL_STATIC_LIB_DIR" LIBSECCOMP_LIB_PATH="$$MUSL_STATIC_LIB_DIR" \
        LIBCAP_LIB_TYPE=static LIBSECCOMP_LIB_TYPE=static \
        cargo build --target x86_64-unknown-linux-musl

test_musl_dir:
	@if [[ ! "$$MUSL_STATIC_LIB_DIR" || ! -d "$$MUSL_STATIC_LIB_DIR" ]]; then\
		echo 'Set MUSL_STATIC_LIB_DIR to a directory containing libcap.a and libseccomp.a statically linked to musl';\
		exit 1;\
	fi
