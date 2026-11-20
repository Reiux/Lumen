use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use flate2::write::GzEncoder;
use flate2::Compression;
use tar::Builder;

fn main() {
    println!("cargo:rerun-if-changed=payload.tar.gz");
    println!("cargo:rerun-if-changed=ui/index.html");
    println!("cargo:rerun-if-env-changed=LUMEN_ALLOW_PLACEHOLDER_PAYLOAD");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_payload = Path::new(&out_dir).join("payload.tar.gz");

    // Check if an external payload.tar.gz is provided in crate root or target
    let local_payload = PathBuf::from("payload.tar.gz");
    if local_payload.exists() {
        let f = File::open(&local_payload).expect("failed to open local payload.tar.gz");
        let gz = flate2::read::GzDecoder::new(f);
        let mut archive = tar::Archive::new(gz);
        let entries = archive
            .entries()
            .expect("local payload.tar.gz is corrupt or truncated (invalid gzip/tar)");
        for entry in entries {
            entry.expect("local payload.tar.gz contains a corrupt/truncated tar entry");
        }
        fs::copy(&local_payload, &out_payload).expect("failed to copy local payload.tar.gz");
    } else {
        if env::var("PROFILE").as_deref() == Ok("release")
            && env::var("LUMEN_ALLOW_PLACEHOLDER_PAYLOAD").as_deref() != Ok("1")
        {
            panic!(
                "Refusing to build release installer without crates/lumen-installer/payload.tar.gz. \
                 Provide payload.tar.gz or set LUMEN_ALLOW_PLACEHOLDER_PAYLOAD=1 to override."
            );
        }

        // Create a dummy valid tar.gz archive for tests and development
        let file = File::create(&out_payload).expect("failed to create placeholder payload.tar.gz");
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = Builder::new(enc);

        let dummy_data = b"Lumen standalone placeholder executable\n";
        let mut header = tar::Header::new_gnu();
        header.set_size(dummy_data.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();

        tar.append_data(&mut header, "lumen.exe", &dummy_data[..])
            .expect("failed to append placeholder lumen.exe");
        tar.append_data(&mut header, "lumen-cli.exe", &dummy_data[..])
            .expect("failed to append placeholder lumen-cli.exe");

        tar.finish().expect("failed to finish placeholder archive");
    }

    tauri_build::build();
}
