//! Permission token enforcement for plugin host API calls.

use crate::error::{AppError, AppResult};

/// All permission tokens recognised in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    ReadAssets,
    ReadMetadata,
    WriteMetadata,
    RenameFilesystem,
    MoveFilesystem,
    CopyFilesystem,
    DeleteFilesystem,
    ExportAssets,
}

impl Permission {
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "read:assets" => Some(Self::ReadAssets),
            "read:metadata" => Some(Self::ReadMetadata),
            "write:metadata" => Some(Self::WriteMetadata),
            "rename:filesystem" => Some(Self::RenameFilesystem),
            "move:filesystem" => Some(Self::MoveFilesystem),
            "copy:filesystem" => Some(Self::CopyFilesystem),
            "delete:filesystem" => Some(Self::DeleteFilesystem),
            "export:assets" => Some(Self::ExportAssets),
            _ => None,
        }
    }
}

/// Parsed set of permissions granted to a plugin.
#[derive(Debug, Clone, Default)]
pub struct PermissionSet(std::collections::HashSet<Permission>);

impl PermissionSet {
    pub fn from_tokens(tokens: &[String]) -> Self {
        let set = tokens
            .iter()
            .filter_map(|t| Permission::from_token(t))
            .collect();
        Self(set)
    }

    pub fn has(&self, perm: Permission) -> bool {
        self.0.contains(&perm)
    }

    /// Returns an error if `perm` is not granted.
    pub fn require(&self, perm: Permission) -> AppResult<()> {
        if self.has(perm) {
            Ok(())
        } else {
            Err(AppError::msg(format!(
                "PLUGIN_PERMISSION_DENIED: {:?} not granted",
                perm
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tokens() {
        let set = PermissionSet::from_tokens(&[
            "read:assets".into(),
            "rename:filesystem".into(),
        ]);
        assert!(set.has(Permission::ReadAssets));
        assert!(set.has(Permission::RenameFilesystem));
        assert!(!set.has(Permission::WriteMetadata));
    }

    #[test]
    fn require_missing_errors() {
        let set = PermissionSet::from_tokens(&[]);
        let err = set.require(Permission::ExportAssets).unwrap_err();
        assert!(err.to_string().contains("PLUGIN_PERMISSION_DENIED"));
    }
}
