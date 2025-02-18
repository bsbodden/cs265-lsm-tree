pub const TOMBSTONE: i64 = i64::MIN;

/// Key type for LSM tree operations
pub type Key = i64;

/// Value type for LSM tree operations
pub type Value = i64;

/// Result type that uses our custom Error
pub type Result<T> = std::result::Result<T, Error>;

/// Custom error types for LSM tree operations
#[derive(Debug)]
pub enum Error {
    /// I/O errors
    Io(std::io::Error),
    /// Key not found in the tree
    KeyNotFound(Key),
    /// Range query with invalid bounds
    InvalidRange { start: Key, end: Key },
    /// Buffer has reached capacity
    BufferFull,
    /// Error during compaction
    CompactionError,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {}", e),
            Error::KeyNotFound(k) => write!(f, "Key not found: {}", k),
            Error::InvalidRange { start, end } => write!(f, "Invalid range: {} > {}", start, end),
            Error::BufferFull => write!(f, "Buffer is full"),
            Error::CompactionError => write!(f, "Error during compaction"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn test_error_display() {
        // Test IO error display
        let io_err = Error::Io(io::Error::new(io::ErrorKind::NotFound, "test error"));
        assert!(io_err.to_string().contains("I/O error"));

        // Test KeyNotFound error
        let key_err = Error::KeyNotFound(42);
        assert_eq!(key_err.to_string(), "Key not found: 42");

        // Test InvalidRange error
        let range_err = Error::InvalidRange { start: 10, end: 5 };
        assert_eq!(range_err.to_string(), "Invalid range: 10 > 5");

        // Test BufferFull error
        let buffer_err = Error::BufferFull;
        assert_eq!(buffer_err.to_string(), "Buffer is full");
    }

    #[test]
    fn test_error_conversion() {
        // Test conversion from io::Error
        let io_err = io::Error::new(io::ErrorKind::Other, "test error");
        let converted: Error = io_err.into();
        matches!(converted, Error::Io(_));
    }

    #[test]
    fn test_result_type() {
        // Test Result with success
        let success: Result<i32> = Ok(42);
        assert_eq!(success.unwrap(), 42);

        // Test Result with error
        let failure: Result<i32> = Err(Error::BufferFull);
        assert!(failure.is_err());
        matches!(failure.unwrap_err(), Error::BufferFull);
    }
}
