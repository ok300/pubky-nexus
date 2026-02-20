use async_trait::async_trait;
use tokio::process::Command;

use crate::{media::FileVariant, models::file::FileDetails, types::DynError};

use super::{BaseProcessingOptions, VariantProcessor};

pub struct VideoOptions {
    width: String,
    format: String,
    content_type: String,
}

impl BaseProcessingOptions for VideoOptions {
    fn content_type(&self) -> String {
        self.content_type.clone()
    }
}

/// VideoProcessor is just a prototype and not a real implementation
/// when we decide to actual start video processing we will need to implement this.
pub struct VideoProcessor;

#[async_trait]
impl VariantProcessor for VideoProcessor {
    type ProcessingOptions = VideoOptions;

    fn get_valid_variants_for_content_type(_content_type: &str) -> Vec<FileVariant> {
        vec![FileVariant::Main]
    }

    fn get_content_type_for_variant(file: &FileDetails, _variant: &FileVariant) -> String {
        file.content_type.clone()
    }

    fn get_options_for_variant(
        _file: &FileDetails,
        _variant: &FileVariant,
    ) -> Result<VideoOptions, DynError> {
        // Return Err until we have a real implementation
        // TODO: Add real implementation for videos
        Err("Not implemented".into())
    }

    async fn process(
        origin_file_path: &str,
        output_file_path: &str,
        options: &VideoOptions,
    ) -> Result<String, DynError> {
        let origin_file_format = VideoProcessor::get_format(origin_file_path).await?;

        let output = match origin_file_format == options.format {
            true => output_file_path.to_string(),
            false => format!("{}.{}", output_file_path, options.format),
        };

        let child_output = match Command::new("ffmpeg")
            .arg("-i")
            .arg(origin_file_path)
            .arg("-vf")
            .arg(format!("scale={}:-1", options.width))
            .arg("-c:a")
            .arg("copy")
            .arg(output)
            .output() // Automatically pipes stdout and stderr
            .await
        {
            Ok(output) => output,
            Err(err) => return Err(err.into()),
        };

        if child_output.status.success() {
            Ok(String::from_utf8_lossy(&child_output.stdout).to_string())
        } else {
            Err(format!(
                "FFmpeg command failed: {}",
                String::from_utf8_lossy(&child_output.stderr)
            )
            .into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::file::FileDetails;

    fn make_file_details(content_type: &str) -> FileDetails {
        FileDetails {
            content_type: content_type.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_get_content_type_for_variant_mp4() {
        let file = make_file_details("video/mp4");
        assert_eq!(
            VideoProcessor::get_content_type_for_variant(&file, &FileVariant::Main),
            "video/mp4"
        );
    }

    #[test]
    fn test_get_content_type_for_variant_webm() {
        let file = make_file_details("video/webm");
        assert_eq!(
            VideoProcessor::get_content_type_for_variant(&file, &FileVariant::Main),
            "video/webm"
        );
    }

    #[test]
    fn test_get_content_type_for_variant_quicktime() {
        let file = make_file_details("video/quicktime");
        assert_eq!(
            VideoProcessor::get_content_type_for_variant(&file, &FileVariant::Main),
            "video/quicktime"
        );
    }
}

impl VideoProcessor {
    // function to get the format of the video
    async fn get_format(input: &str) -> Result<String, DynError> {
        let child_output = Command::new("ffmpeg")
            .arg("-i")
            .arg(input)
            .arg("-f")
            .arg("null")
            .output() // Automatically pipes stdout and stderr
            .await?;

        if child_output.status.success() {
            Ok(String::from_utf8_lossy(&child_output.stdout).to_string())
        } else {
            Err(format!(
                "FFmpeg metadata extraction failed: {}",
                String::from_utf8_lossy(&child_output.stderr)
            )
            .into())
        }
    }
}
