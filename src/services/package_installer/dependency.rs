// src/package_installer/dependency.rs
use thiserror::Error;

use crate::{
    domain::config::AppConfig,
    domain::dependency::{DependencyGraph, DependencyGraphError},
    domain::package::Package,
    ports::package_repo::{PackageRepoError, PackageRepository},
};

#[derive(Error, Debug)]
pub enum DependencyResolverError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Multiple packages found with name: {0}")]
    MultiplePackagesFound(String),

    #[error("Repository error: {0}")]
    RepoError(#[from] PackageRepoError),

    #[error("Dependency graph error: {0}")]
    GraphError(#[from] DependencyGraphError),

    #[error("Environment {0} not supported by package {1}")]
    EnvironmentNotSupported(String, String),
}

pub struct DependencyResolver<'a, P: PackageRepository> {
    package_repo: &'a P,
    config: &'a AppConfig,
}

impl<'a, P: PackageRepository> DependencyResolver<'a, P> {
    pub fn new(package_repo: &'a P, config: &'a AppConfig) -> Self {
        Self {
            package_repo,
            config,
        }
    }

    /// Resolve dependencies for a package and return an ordered list of packages
    /// that need to be installed
    pub fn resolve_dependencies(
        &self,
        package_name: &str,
    ) -> Result<Vec<Package>, DependencyResolverError> {
        // Build the dependency graph starting with the requested package
        let mut graph = DependencyGraph::default();
        self.build_dependency_graph(&mut graph, package_name, &mut Vec::new())?;

        // Get the installation order
        let installation_order = match graph.installation_order() {
            Ok(order) => order,
            Err(DependencyGraphError::CircularDependency(msg, path)) => {
                // Convert the graph error to our error type with the cycle path
                return Err(DependencyResolverError::CircularDependency(format!(
                    "{} (path: {})",
                    msg,
                    path.join(" -> ")
                )));
            }
            Err(e) => return Err(DependencyResolverError::GraphError(e)),
        };

        Ok(installation_order.into_iter().cloned().collect())
    }

    /// Recursively build the dependency graph
    fn build_dependency_graph(
        &self,
        graph: &mut DependencyGraph,
        package_name: &str,
        visited: &mut Vec<String>,
    ) -> Result<(), DependencyResolverError> {
        // Check for circular dependencies during traversal
        if visited.contains(&package_name.to_string()) {
            let mut cycle_path = visited.clone();
            cycle_path.push(package_name.to_string());

            return Err(DependencyResolverError::CircularDependency(format!(
                "Circular dependency detected: {}",
                visited.join(" -> ") + " -> " + package_name
            )));
        }

        // Get the package
        let package = self
            .package_repo
            .get_package(package_name)
            .map_err(|e| match e {
                PackageRepoError::PackageNotFound(name) => {
                    DependencyResolverError::PackageNotFound(name)
                }
                PackageRepoError::MultiplePackagesFound(name) => {
                    DependencyResolverError::MultiplePackagesFound(name)
                }
                other => DependencyResolverError::RepoError(other),
            })?;

        // Get environment configuration for this package
        let env_config = self.config.resolve_environment(&package).map_err(|_| {
            DependencyResolverError::EnvironmentNotSupported(
                self.config.environment().to_string(),
                package.name.clone(),
            )
        })?;

        // Add the package to the graph if not already added
        if !graph
            .get_package_names()
            .contains(&package.name.to_string())
        {
            graph.add_node(package.clone())?;
        }

        // Process dependencies
        visited.push(package_name.to_string());

        for dep_name in &env_config.dependencies {
            // Get dependency package
            let dep_package = self
                .package_repo
                .get_package(dep_name)
                .map_err(|e| match e {
                    PackageRepoError::PackageNotFound(name) => {
                        DependencyResolverError::PackageNotFound(name)
                    }
                    PackageRepoError::MultiplePackagesFound(name) => {
                        DependencyResolverError::MultiplePackagesFound(name)
                    }
                    other => DependencyResolverError::RepoError(other),
                })?;

            // Add dependency node if not already in the graph
            if !graph
                .get_package_names()
                .contains(&dep_package.name.to_string())
            {
                graph.add_node(dep_package.clone())?;
            }

            // Add dependency relationship
            graph.add_dependency(&package.name, dep_name)?;

            // Recursively process this dependency
            let mut dep_visited = visited.clone();
            self.build_dependency_graph(graph, dep_name, &mut dep_visited)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::config::AppConfigBuilder, ports::package_repo::MockPackageRepository};

    fn setup_test_environment() -> (MockPackageRepository, AppConfig) {
        let package_repo = MockPackageRepository::new();

        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        (package_repo, config)
    }

    fn create_test_package(name: &str, version: &str, dependencies: &[&str]) -> Package {
        let mut yaml = format!(
            r#"
name: {}
version: {}
environments:
  test-env:
    install: echo "Installing {}"
"#,
            name, version, name
        );

        if !dependencies.is_empty() {
            yaml.push_str("    dependencies:\n");
            for dep in dependencies {
                yaml.push_str(&format!("      - {}\n", dep));
            }
        }

        Package::from_yaml(&yaml).unwrap()
    }

    #[test]
    fn test_resolve_simple_dependency() {
        let (mut package_repo, config) = setup_test_environment();

        // Add package and dependency to the filesystem
        let package = create_test_package("main-pkg", "1.0.0", &["dep-pkg"]);
        let dep = create_test_package("dep-pkg", "1.0.0", &[]);

        package_repo.mock_get_package_ok("main-pkg", package);
        package_repo.mock_get_package_ok("dep-pkg", dep);

        // Create resolver and resolve dependencies
        let resolver = DependencyResolver::new(&package_repo, &config);
        let result = resolver.resolve_dependencies("main-pkg");

        assert!(result.is_ok());
        let packages = result.unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "dep-pkg"); // Dependency should be first
        assert_eq!(packages[1].name, "main-pkg"); // Main package should be last
    }

    #[test]
    fn test_resolve_deep_dependency_chain() {
        let (mut package_repo, config) = setup_test_environment();

        // Create a chain: main -> dep1 -> dep2 -> dep3
        let main = create_test_package("main-pkg", "1.0.0", &["dep1"]);
        let dep1 = create_test_package("dep1", "1.0.0", &["dep2"]);
        let dep2 = create_test_package("dep2", "1.0.0", &["dep3"]);
        let dep3 = create_test_package("dep3", "1.0.0", &[]);

        package_repo.mock_get_package_ok("main-pkg", main);
        package_repo.mock_get_package_ok("dep1", dep1);
        package_repo.mock_get_package_ok("dep2", dep2);
        package_repo.mock_get_package_ok("dep3", dep3);

        // Create resolver and resolve dependencies
        let resolver = DependencyResolver::new(&package_repo, &config);
        let result = resolver.resolve_dependencies("main-pkg");

        assert!(result.is_ok());
        let packages = result.unwrap();
        assert_eq!(packages.len(), 4);

        // Check order: deepest dependencies first
        assert_eq!(packages[0].name, "dep3");
        assert_eq!(packages[1].name, "dep2");
        assert_eq!(packages[2].name, "dep1");
        assert_eq!(packages[3].name, "main-pkg");
    }

    #[test]
    fn test_resolve_diamond_dependency() {
        let (mut package_repo, config) = setup_test_environment();

        // Create a diamond: main -> (dep1, dep2) -> common-dep
        let main = create_test_package("main-pkg", "1.0.0", &["dep1", "dep2"]);
        let dep1 = create_test_package("dep1", "1.0.0", &["common-dep"]);
        let dep2 = create_test_package("dep2", "1.0.0", &["common-dep"]);
        let common = create_test_package("common-dep", "1.0.0", &[]);

        package_repo.mock_get_package_ok("main-pkg", main);
        package_repo.mock_get_package_ok("dep1", dep1);
        package_repo.mock_get_package_ok("dep2", dep2);
        package_repo.mock_get_package_ok("common-dep", common);

        // Create resolver and resolve dependencies
        let resolver = DependencyResolver::new(&package_repo, &config);
        let result = resolver.resolve_dependencies("main-pkg");

        assert!(result.is_ok());
        let packages = result.unwrap();

        // Should deduplicate common dependency
        assert_eq!(packages.len(), 4);

        // Common dependency should appear first, then dep1 and dep2 (order between them doesn't matter), then main
        assert!(packages.iter().any(|p| p.name == "common-dep"));
        assert!(packages.iter().any(|p| p.name == "dep1"));
        assert!(packages.iter().any(|p| p.name == "dep2"));
        assert_eq!(packages.last().unwrap().name, "main-pkg");
    }

    #[test]
    fn test_detect_circular_dependency() {
        let (mut package_repo, config) = setup_test_environment();

        // Create a circular dependency: main -> dep1 -> main
        let main = create_test_package("main-pkg", "1.0.0", &["dep1"]);
        let dep1 = create_test_package("dep1", "1.0.0", &["main-pkg"]);

        package_repo.mock_get_package_ok("main-pkg", main);
        package_repo.mock_get_package_ok("dep1", dep1);

        // Create resolver and resolve dependencies
        let resolver = DependencyResolver::new(&package_repo, &config);
        let result = resolver.resolve_dependencies("main-pkg");

        assert!(result.is_err());
        match result {
            Err(DependencyResolverError::GraphError(DependencyGraphError::CircularDependency(
                _,
                _,
            ))) => {
                // Expected error - the circular dependency was detected in the graph component
            }
            Err(DependencyResolverError::CircularDependency(_)) => {
                // Also acceptable - the circular dependency was detected in the resolver itself
            }
            other => panic!("Expected circular dependency error; got {:?}", other),
        }
    }

    #[test]
    fn test_dependency_not_found() {
        let (mut package_repo, config) = setup_test_environment();

        // Create a package with a non-existent dependency
        let package = create_test_package("main-pkg", "1.0.0", &["missing-dep"]);

        // Create resolver and resolve dependencies
        package_repo.mock_get_package_ok("main-pkg", package);
        package_repo.mock_get_package_err(
            "missing-dep",
            PackageRepoError::PackageNotFound("missing-dep".to_string()),
        );

        let resolver = DependencyResolver::new(&package_repo, &config);
        let result = resolver.resolve_dependencies("main-pkg");

        assert!(result.is_err());
        match result {
            Err(DependencyResolverError::PackageNotFound(name)) => {
                assert_eq!(name, "missing-dep");
            }
            _ => panic!("Expected package not found error"),
        }
    }

    #[test]
    fn test_environment_not_supported() {
        let (mut package_repo, config) = setup_test_environment();

        // Create a package with a dependency that doesn't support the current environment
        let main_yaml = create_test_package("main-pkg", "1.0.0", &["dep1"]);

        // Create a dependency with a different environment
        let dep1_yaml = r#"
name: dep1
version: 1.0.0
environments:
  different-env:
    install: echo "Installing dep1"
"#;
        let dep1 = Package::from_yaml(dep1_yaml).unwrap();

        package_repo.mock_get_package_ok("main-pkg", main_yaml);
        package_repo.mock_get_package_ok("dep1", dep1);

        // Create resolver and resolve dependencies
        let resolver = DependencyResolver::new(&package_repo, &config);
        let result = resolver.resolve_dependencies("main-pkg");

        assert!(result.is_err());
        match result {
            Err(DependencyResolverError::EnvironmentNotSupported(env, name)) => {
                assert_eq!(env, "test-env");
                assert_eq!(name, "dep1");
            }
            _ => panic!("Expected environment not supported error"),
        }
    }
}
