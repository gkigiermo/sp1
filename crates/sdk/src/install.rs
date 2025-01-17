use cfg_if::cfg_if;
use std::path::PathBuf;

#[cfg(feature = "network")]
use {
    crate::block_on,
    futures::StreamExt,
    indicatif::{ProgressBar, ProgressStyle},
    reqwest::Client,
    std::{cmp::min, io::Write, process::Command},
};

use crate::SP1_CIRCUIT_VERSION;

/// The base URL for the S3 bucket containing the ciruit artifacts.
pub const CIRCUIT_ARTIFACTS_URL_BASE: &str = "https://sp1-circuits.s3-us-east-2.amazonaws.com";

/// Gets the directory where the circuit artifacts are installed.
fn circuit_artifacts_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(".sp1").join("circuits").join(SP1_CIRCUIT_VERSION)
}

/// Tries to install the circuit artifacts if they are not already installed.
pub fn try_install_circuit_artifacts() -> PathBuf {
    let build_dir = circuit_artifacts_dir();

    if build_dir.exists() {
        println!(
            "[sp1] circuit artifacts already seem to exist at {}. if you want to re-download them, delete the directory",
            build_dir.display()
        );
    } else {
        cfg_if! {
            if #[cfg(feature = "network")] {
                println!(
                    "[sp1] circuit artifacts for version {} do not exist at {}. downloading...",
                    SP1_CIRCUIT_VERSION,
                    build_dir.display()
                );
                install_circuit_artifacts(build_dir.clone());
            }
        }
    }
    build_dir
}

/// Install the latest circuit artifacts.
///
/// This function will download the latest circuit artifacts from the S3 bucket and extract them
/// to the directory specified by [plonk_bn254_artifacts_dir()].
#[cfg(feature = "network")]
pub fn install_circuit_artifacts(build_dir: PathBuf) {
    // Create the build directory.
    std::fs::create_dir_all(&build_dir).expect("failed to create build directory");

    // Download the artifacts.
    let download_url = format!("{}/{}.tar.gz", CIRCUIT_ARTIFACTS_URL_BASE, SP1_CIRCUIT_VERSION);
    let mut artifacts_tar_gz_file =
        tempfile::NamedTempFile::new().expect("failed to create tempfile");
    let client = Client::builder().build().expect("failed to create reqwest client");
    block_on(download_file(&client, &download_url, &mut artifacts_tar_gz_file))
        .expect("failed to download file");

    // Extract the tarball to the build directory.
    let mut res = Command::new("tar")
        .args([
            "-Pxzf",
            artifacts_tar_gz_file.path().to_str().unwrap(),
            "-C",
            build_dir.to_str().unwrap(),
        ])
        .spawn()
        .expect("failed to extract tarball");
    res.wait().unwrap();

    println!("[sp1] downloaded {} to {:?}", download_url, build_dir.to_str().unwrap(),);
}

/// The directory where the circuit artifacts will be stored.
pub fn install_circuit_artifacts_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(".sp1").join("circuits").join(SP1_CIRCUIT_VERSION)
}

/// Download the file with a progress bar that indicates the progress.
#[cfg(feature = "network")]
pub async fn download_file(
    client: &Client,
    url: &str,
    file: &mut tempfile::NamedTempFile,
) -> std::result::Result<(), String> {
    let res = client.get(url).send().await.or(Err(format!("Failed to GET from '{}'", &url)))?;

    let total_size =
        res.content_length().ok_or(format!("Failed to get content length from '{}'", &url))?;

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})").unwrap()
        .progress_chars("#>-"));

    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();
    while let Some(item) = stream.next().await {
        let chunk = item.or(Err("Error while downloading file"))?;
        file.write_all(&chunk).or(Err("Error while writing to file"))?;
        let new = min(downloaded + (chunk.len() as u64), total_size);
        downloaded = new;
        pb.set_position(new);
    }
    pb.finish();

    Ok(())
}
