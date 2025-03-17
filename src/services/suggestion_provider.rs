// src/services/suggestion_provider.rs
// Provides "did you mean" suggestions for various types of errors

use std::path::{Path, PathBuf};

use crate::ports::filesystem::FileSystem;
use crate::ports::package_repo::PackageRepository;

/// Generates suggestions for unknown names or values
pub(crate) struct SuggestionProvider<'a> {
    fs: &'a dyn FileSystem,
    package_repo: &'a dyn PackageRepository,
}

impl<'a> SuggestionProvider<'a> {
    /// Create a new suggestion provider
    pub(crate) fn new(fs: &'a dyn FileSystem, package_repo: &'a dyn PackageRepository) -> Self {
        Self { fs, package_repo }
    }

    /// Get suggestions for a package name
    pub(crate) fn suggest_package(&self, name: &str) -> Vec<String> {
        // Try to get packages from the repository
        match self.package_repo.list_packages() {
            Ok(packages) => {
                let package_names: Vec<String> = packages.iter().map(|p| p.name.clone()).collect();
                self.find_similar_strings(name, &package_names)
            }
            Err(_) => Vec::new(),
        }
    }

    /// Get suggestions for a file path
    pub(crate) fn suggest_path(&self, path: &Path) -> Vec<PathBuf> {
        // If the path doesn't exist, suggest similar paths in the parent directory
        if !self.fs.path_exists(path) {
            if let Some(parent) = path.parent() {
                if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                    // Only try to find suggestions if parent directory exists
                    if self.fs.path_exists(parent) {
                        if let Ok(entries) = self.fs.list_directory(parent) {
                            let entry_names: Vec<String> = entries
                                .iter()
                                .filter_map(|p| {
                                    p.file_name().and_then(|f| f.to_str().map(String::from))
                                })
                                .collect();

                            let similar = self.find_similar_strings(filename, &entry_names);
                            return similar.iter().map(|name| parent.join(name)).collect();
                        }
                    }
                }
            }
        }

        Vec::new()
    }

    /// Get suggestions for an environment name
    pub(crate) fn suggest_environment(
        &self,
        name: &str,
        known_environments: &[String],
    ) -> Vec<String> {
        self.find_similar_strings(name, known_environments)
    }

    /// Get suggestions for known environment variables
    pub(crate) fn suggest_env_var(&self, name: &str) -> Vec<String> {
        // Common environment variables that might be used in package definitions
        let common_vars = [
            "HOME",
            "PATH",
            "USER",
            "SHELL",
            "TERM",
            "LANG",
            "PWD",
            "DISPLAY",
            "EDITOR",
            "VISUAL",
            "PAGER",
            "BROWSER",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
        ];

        let vars: Vec<String> = common_vars.iter().map(|&s| s.to_string()).collect();
        self.find_similar_strings(name, &vars)
    }

    /// Find similar strings using string similarity
    fn find_similar_strings(&self, target: &str, candidates: &[String]) -> Vec<String> {
        const MAX_SUGGESTIONS: usize = 3;

        // Calculate similarity for all candidates
        let mut similarities: Vec<(String, f64)> = candidates
            .iter()
            .map(|candidate| {
                let similarity = self.calculate_similarity(target, candidate);
                (candidate.clone(), similarity)
            })
            .filter(|(_, similarity)| *similarity > 0.53)
            .collect();

        // Sort by similarity (most similar first)
        similarities
            .sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

        // Take top suggestions
        similarities
            .into_iter()
            .take(MAX_SUGGESTIONS)
            .map(|(name, _)| name)
            .collect()
    }

    /// Calculate string similarity
    fn calculate_similarity(&self, s1: &str, s2: &str) -> f64 {
        strsim::jaro_winkler(s1, s2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::package::PackageBuilder;
    use crate::ports::filesystem::MockFileSystem;
    use crate::ports::package_repo::MockPackageRepository;

    #[test]
    fn test_suggest_package() {
        let fs = MockFileSystem::default();
        let mut package_repo = MockPackageRepository::new();

        // Create test packages
        let packages = vec![
            PackageBuilder::default()
                .name("ripgrep")
                .version("1.0.0")
                .build(),
            PackageBuilder::default()
                .name("ripgrep-all")
                .version("1.0.0")
                .build(),
            PackageBuilder::default()
                .name("fzf")
                .version("1.0.0")
                .build(),
            PackageBuilder::default()
                .name("bat")
                .version("1.0.0")
                .build(),
        ];

        package_repo
            .expect_list_packages()
            .returning(move || Ok(packages.clone()));

        let provider = SuggestionProvider::new(&fs, &package_repo);

        // Test with similar name
        let suggestions = provider.suggest_package("rigrep");
        assert!(!suggestions.is_empty());
        assert!(suggestions.contains(&"ripgrep".to_string()));

        // Test with unrelated name
        let suggestions = provider.suggest_package("completely-different");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_path() {
        let mut fs = MockFileSystem::default();
        let package_repo = MockPackageRepository::new();

        // Mock file system with some paths
        let parent = Path::new("/test/dir");
        fs.mock_path_exists(&parent, true);
        // fs.add_existing_path(parent);

        let file1 = parent.join("file1.txt");
        let file2 = parent.join("file2.txt");
        let file3 = parent.join("other.txt");
        fs.mock_path_exists(&file1, true);
        fs.mock_path_exists(&file2, true);
        fs.mock_path_exists(&file3, true);
        fs.mock_list_directory(
            &parent,
            &[&file1.as_path(), &file2.as_path(), &file3.as_path()],
        );

        fs.mock_path_exists(parent.join("flie1.txt"), false);
        fs.mock_path_exists(parent.join("xyz.txt"), true);

        let provider = SuggestionProvider::new(&fs, &package_repo);

        // Test with similar filename
        let suggestions = provider.suggest_path(&parent.join("flie1.txt"));
        assert!(!suggestions.is_empty());
        assert!(suggestions.contains(&file1));

        // Test with unrelated filename
        let suggestions = provider.suggest_path(&parent.join("xyz.txt"));
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_environment() {
        let fs = MockFileSystem::default();
        let package_repo = MockPackageRepository::new();
        let provider = SuggestionProvider::new(&fs, &package_repo);

        let envs = vec![
            "production".to_string(),
            "development".to_string(),
            "testing".to_string(),
            "staging".to_string(),
        ];

        // Test with similar environment name
        let suggestions = provider.suggest_environment("prod", &envs);
        assert!(!suggestions.is_empty());
        assert!(suggestions.contains(&"production".to_string()));

        // Test with unrelated environment name
        let suggestions = provider.suggest_environment("xyz", &envs);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_calculate_similarity() {
        let fs = MockFileSystem::default();
        let package_repo = MockPackageRepository::new();
        let provider = SuggestionProvider::new(&fs, &package_repo);

        // Exact match
        assert_eq!(provider.calculate_similarity("apple", "apple"), 1.0);

        // Case-insensitive match
        assert_eq!(
            provider.calculate_similarity("Apple", "apple"),
            0.866_666_666_666_666_7
        );

        // Prefix match
        assert_eq!(
            provider.calculate_similarity("app", "apple"),
            0.906_666_666_666_666_7
        );

        // Contains match
        assert_eq!(
            provider.calculate_similarity("ple", "apple"),
            0.511_111_111_111_111_1
        );

        // Partial similarity
        assert!(provider.calculate_similarity("aple", "apple") > 0.53);

        // Low similarity
        assert!(provider.calculate_similarity("xyz", "apple") < 0.53);
    }
}
