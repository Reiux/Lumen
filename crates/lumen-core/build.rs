use std::io::Result;

fn main() -> Result<()> {
    let mut config = prost_build::Config::new();
    config.compile_protos(
        &[
            "../../proto/AddMediaToAlbum.proto",
            "../../proto/CommitToken.proto",
            "../../proto/CommitUpload.proto",
            "../../proto/CommitUploadResponse.proto",
            "../../proto/CreateAlbum.proto",
            "../../proto/CreateAlbumResponse.proto",
            "../../proto/CreateMediaItems.proto",
            "../../proto/GetUploadToken.proto",
            "../../proto/HashCheck.proto",
            "../../proto/RemoteMatches.proto",
        ],
        &["../../proto"],
    )?;
    Ok(())
}
