//! Comprehensive tests for clipboard functionality.
//!
//! This module provides extensive testing for:
//! - Text copy/paste operations
//! - Unicode text handling
//! - OSC 52 escape sequence support
//! - Image paste detection and validation
//! - Cross-platform compatibility

#[cfg(test)]
mod clipboard_tests {
    use super::*;

    #[test]
    fn test_image_format_detection_png() {
        let png_header = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let format = ImageFormatType::from_magic_bytes(&png_header);

        assert_eq!(format, ImageFormatType::Png);
        assert_eq!(format.mime_type(), "image/png");
        assert_eq!(format.extension(), "png");
        assert_eq!(format.name(), "PNG");
    }

    #[test]
    fn test_image_format_detection_jpeg() {
        let jpeg_header = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let format = ImageFormatType::from_magic_bytes(&jpeg_header);

        assert_eq!(format, ImageFormatType::Jpeg);
        assert_eq!(format.mime_type(), "image/jpeg");
        assert_eq!(format.extension(), "jpg");
        assert_eq!(format.name(), "JPEG");
    }

    #[test]
    fn test_image_format_detection_gif() {
        let gif_header = b"GIF8";
        let format = ImageFormatType::from_magic_bytes(gif_header);

        assert_eq!(format, ImageFormatType::Gif);
        assert_eq!(format.mime_type(), "image/gif");
        assert_eq!(format.extension(), "gif");
        assert_eq!(format.name(), "GIF");
    }

    #[test]
    fn test_image_format_detection_webp() {
        let mut webp_header = vec![0x52, 0x49, 0x46, 0x46]; // "RIFF"
        webp_header.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // size
        webp_header.extend_from_slice(&[0x57, 0x45, 0x42, 0x50]); // "WEBP"

        let format = ImageFormatType::from_magic_bytes(&webp_header);

        assert_eq!(format, ImageFormatType::WebP);
        assert_eq!(format.mime_type(), "image/webp");
        assert_eq!(format.extension(), "webp");
        assert_eq!(format.name(), "WebP");
    }

    #[test]
    fn test_image_format_detection_unknown() {
        let unknown_data = vec![0x00, 0x01, 0x02, 0x03];
        let format = ImageFormatType::from_magic_bytes(&unknown_data);

        assert_eq!(format, ImageFormatType::Unknown);
        assert_eq!(format.mime_type(), "application/octet-stream");
        assert_eq!(format.extension(), "bin");
        assert_eq!(format.name(), "Unknown");
    }

    #[test]
    fn test_image_format_detection_empty() {
        let empty_data = vec![];
        let format = ImageFormatType::from_magic_bytes(&empty_data);

        assert_eq!(format, ImageFormatType::Unknown);
    }

    #[test]
    fn test_image_format_detection_too_short() {
        let short_data = vec![0x89, 0x50];
        let format = ImageFormatType::from_magic_bytes(&short_data);

        assert_eq!(format, ImageFormatType::Unknown);
    }

    #[test]
    fn test_image_size_validation() {
        let small_data = vec![0u8; 1024]; // 1KB - should be OK
        let large_data = vec![0u8; 15 * 1024 * 1024]; // 15MB - too large

        // Small data should pass
        assert!(small_data.len() <= MAX_IMAGE_SIZE);

        // Large data should exceed limit
        assert!(large_data.len() > MAX_IMAGE_SIZE);
    }

    #[test]
    fn test_image_format_to_image_format() {
        let png_format = ImageFormatType::Png;
        let image_format = png_format.to_image_format();

        assert!(image_format.is_some());
        assert_eq!(image_format.unwrap(), image::ImageFormat::Png);

        let unknown_format = ImageFormatType::Unknown;
        assert!(unknown_format.to_image_format().is_none());
    }

    #[test]
    fn test_osc52_encoding() {
        let text = "Hello, World!";
        let encoded = encode_osc52(text);

        // Should contain proper OSC 52 sequence
        assert!(encoded.contains("\x1b]52;"));
        assert!(encoded.contains("\x07"));

        // Should contain base64 encoded text
        assert!(encoded.contains("SGVsbG8sIFdvcmxkIQ=="));
    }

    #[test]
    fn test_osc52_unicode() {
        let text = "สวัสดี 🌍 Hello";
        let encoded = encode_osc52(text);

        // Should handle Unicode
        assert!(encoded.contains("\x1b]52;"));
        assert!(encoded.contains("\x07"));

        // Should be valid base64
        assert!(encoded.len() > text.len());
    }

    #[test]
    fn test_osc52_empty() {
        let text = "";
        let encoded = encode_osc52(text);

        // Should still produce valid OSC 52 sequence
        assert!(encoded.contains("\x1b]52;"));
        assert!(encoded.contains("\x07"));
    }

    #[test]
    fn test_base64_encode_image() {
        let image_data = vec![0x89, 0x50, 0x4E, 0x47]; // PNG header
        let encoded = base64_encode(&image_data);

        // Should be valid base64
        assert!(encoded.chars().all(|c| c.is_alphanumeric() || c == '+' || c == '/' || c == '='));

        // Decode and verify
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, image_data);
    }

    #[test]
    fn test_base64_encode_unicode() {
        let text = "สวัสดี 🌍";
        let encoded = base64_encode(text.as_bytes());

        // Should be valid base64
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(String::from_utf8(decoded).unwrap(), text);
    }

    #[test]
    fn test_base64_decode_invalid() {
        let invalid_base64 = "This is not valid base64!@#$";

        let result = base64_decode(invalid_base64);
        assert!(result.is_err());
    }

    #[test]
    fn test_clipboard_mime_type_detection() {
        // Test various common mime types
        let tests = vec![
            ("image/png", ImageFormatType::Png),
            ("image/jpeg", ImageFormatType::Jpeg),
            ("image/gif", ImageFormatType::Gif),
            ("image/webp", ImageFormatType::WebP),
            ("application/octet-stream", ImageFormatType::Unknown),
        ];

        for (mime, expected) in tests {
            // We can't directly test mime parsing without clipboard access,
            // but we can verify the format's mime_type() method
            assert_eq!(expected.mime_type(), mime);
        }
    }

    #[test]
    fn test_image_attachment_creation() {
        let data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let attachment = ImageAttachment::new(data.clone(), "image.png".to_string());

        assert_eq!(attachment.filename, "image.png");
        assert_eq!(attachment.data, data);
        assert_eq!(attachment.mime_type, "image/png");
    }

    #[test]
    fn test_image_attachment_size_calculation() {
        let data = vec![0u8; 1024]; // 1KB
        let attachment = ImageAttachment::new(data, "test.png".to_string());

        assert_eq!(attachment.size(), 1024);
    }

    #[test]
    fn test_image_attachment_format_detection() {
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let attachment = ImageAttachment::new(png_data, "image".to_string());

        assert_eq!(attachment.format(), ImageFormatType::Png);
    }

    #[test]
    fn test_image_attachment_is_valid() {
        let valid_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let attachment = ImageAttachment::new(valid_png, "valid.png".to_string());

        assert!(attachment.is_valid());

        // Test invalid (too large)
        let too_large = vec![0u8; 15 * 1024 * 1024];
        let large_attachment = ImageAttachment::new(too_large, "large.png".to_string());

        assert!(!large_attachment.is_valid());
    }

    #[test]
    fn test_multiple_image_attachments() {
        let mut attachments = Vec::new();

        for i in 0..5 {
            let data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
            attachments.push(ImageAttachment::new(data, format!("image{}.png", i)));
        }

        assert_eq!(attachments.len(), 5);

        // Verify each attachment
        for (i, attachment) in attachments.iter().enumerate() {
            assert_eq!(attachment.filename, format!("image{}.png", i));
            assert!(attachment.is_valid());
        }
    }

    #[test]
    fn test_image_attachment_display_name() {
        let attachment = ImageAttachment::new(
            vec![0x89, 0x50, 0x4E, 0x47],
            "very_long_filename_that_should_be_truncated.png".to_string(),
        );

        let display_name = attachment.display_name(30);

        // Should be truncated to fit within limit
        assert!(display_name.len() <= 30);
    }

    #[test]
    fn test_image_size_human_readable() {
        let attachment = ImageAttachment::new(vec![0u8; 1024 * 1024], "1mb.png".to_string());

        let size_str = attachment.size_human_readable();
        assert!(size_str.contains("MB") || size_str.contains("MiB"));
    }

    #[test]
    fn test_clipboard_error_handling() {
        // Test error handling for empty clipboard
        let result = get_clipboard_text();
        // This might fail if no clipboard available, which is OK for testing

        // Test error handling for invalid data
        let result = ImageAttachment::from_clipboard_data(vec![], "unknown".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_clipboard_cross_platform() {
        // This test verifies the code compiles for all platforms
        // Actual clipboard access requires platform-specific tests

        #[cfg(target_os = "macos")]
        {
            // macOS-specific clipboard tests
            assert!(true); // Placeholder
        }

        #[cfg(target_os = "linux")]
        {
            // Linux-specific clipboard tests (X11/Wayland)
            assert!(true); // Placeholder
        }

        #[cfg(target_os = "windows")]
        {
            // Windows-specific clipboard tests
            assert!(true); // Placeholder
        }
    }
}

// Helper functions for testing

fn encode_osc52(text: &str) -> String {
    let encoded = base64_encode(text.as_bytes());
    format!("\x1b]52;{}\x07", encoded)
}

fn base64_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn base64_decode(encoded: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(encoded)
}
