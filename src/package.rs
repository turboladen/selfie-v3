// src/package.rs

// impl PackageNode {
// Parse a PackageNode from YAML string
// pub fn from_yaml(yaml_str: &str) -> Result<Self, PackageParseError> {
//     let mut package: PackageNode = serde_yaml::from_str(yaml_str)?;
//
//     // Ensure defaults are set
//     for env_config in package.environments.values_mut() {
//         if env_config.dependencies.is_empty() {
//             env_config.dependencies = Vec::new();
//         }
//     }
//
//     Ok(package)
// }
//
// // Load a PackageNode from a file using the FileSystem trait
// pub fn from_file<F: crate::filesystem::FileSystem>(
//     fs: &F,
//     path: &Path,
// ) -> Result<Self, PackageParseError> {
//     let content = fs
//         .read_file(path)
//         .map_err(|e| PackageParseError::FileSystemError(e.to_string()))?;
//
//     let mut package = Self::from_yaml(&content)?;
//     package.path = Some(path.to_path_buf());
//
//     Ok(package)
// }
//
// // Serialize to YAML
// pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
//     serde_yaml::to_string(self)
// }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::filesystem::mock::MockFileSystem;
//     use std::path::Path;
//
//     #[test]
//     fn test_package_from_yaml() {
//         let yaml = r#"
//             name: ripgrep
//             version: 0.1.0
//             homepage: https://example.com
//             description: Fast line-oriented search tool
//             environments:
//               mac:
//                 install: brew install ripgrep
//                 check: which rg
//                 dependencies:
//                   - brew
//               linux:
//                 install: apt install ripgrep
//         "#;
//
//         let package = PackageNode::from_yaml(yaml).unwrap();
//
//         assert_eq!(package.name, "ripgrep");
//         assert_eq!(package.version, "0.1.0");
//         assert_eq!(package.homepage, Some("https://example.com".to_string()));
//         assert_eq!(
//             package.description,
//             Some("Fast line-oriented search tool".to_string())
//         );
//         assert_eq!(package.environments.len(), 2);
//         assert_eq!(
//             package.environments.get("mac").unwrap().install,
//             "brew install ripgrep"
//         );
//         assert_eq!(
//             package.environments.get("mac").unwrap().check,
//             Some("which rg".to_string())
//         );
//         assert_eq!(
//             package.environments.get("mac").unwrap().dependencies,
//             vec!["brew"]
//         );
//         assert_eq!(
//             package.environments.get("linux").unwrap().install,
//             "apt install ripgrep"
//         );
//         assert_eq!(package.environments.get("linux").unwrap().check, None);
//         assert!(package
//             .environments
//             .get("linux")
//             .unwrap()
//             .dependencies
//             .is_empty());
//     }
//
//     #[test]
//     fn test_package_to_yaml() {
//         let package = PackageNodeBuilder::default()
//             .name("ripgrep")
//             .version("0.1.0")
//             .environment_with_check("mac", "brew install ripgrep", "which rg")
//             .environment_with_dependencies("linux", "apt install ripgrep", vec!["apt"])
//             .build();
//
//         let yaml = package.to_yaml().unwrap();
//         let parsed_package = PackageNode::from_yaml(&yaml).unwrap();
//
//         assert_eq!(package.name, parsed_package.name);
//         assert_eq!(package.version, parsed_package.version);
//         assert_eq!(
//             package.environments.len(),
//             parsed_package.environments.len()
//         );
//     }
//
//     #[test]
//     fn test_package_from_file() {
//         let fs = MockFileSystem::default();
//         let path = Path::new("/test/packages/ripgrep.yaml");
//
//         let yaml = r#"
//             name: ripgrep
//             version: 0.1.0
//             environments:
//               mac:
//                 install: brew install ripgrep
//                 check: which rg
//                 dependencies:
//                   - brew
//               linux:
//                 install: apt install ripgrep
//         "#;
//
//         fs.add_file(path, yaml);
//
//         let package = PackageNode::from_file(&fs, path).unwrap();
//
//         assert_eq!(package.name, "ripgrep");
//         assert_eq!(package.version, "0.1.0");
//         assert_eq!(package.environments.len(), 2);
//         assert_eq!(package.path, Some(path.to_path_buf()));
//     }
//
//     #[test]
//     fn test_package_from_file_not_found() {
//         let fs = MockFileSystem::default();
//         let path = Path::new("/test/packages/nonexistent.yaml");
//
//         let result = PackageNode::from_file(&fs, path);
//         assert!(result.is_err());
//     }
//
//     #[test]
//     fn test_package_from_invalid_yaml() {
//         let yaml = r#"
//             name: ripgrep
//             version: 0.1.0
//             environments:
//               - this is invalid YAML for our structure
//         "#;
//
//         let result = PackageNode::from_yaml(yaml);
//         assert!(result.is_err());
//     }
// }
