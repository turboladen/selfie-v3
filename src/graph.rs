// src/graph.rs

use std::collections::{HashMap, HashSet};
use thiserror::Error;

use crate::package::PackageNode;

#[derive(Debug, Error)]
pub enum DependencyGraphError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Duplicate package: {0}")]
    DuplicatePackage(String),

    #[error("Invalid dependency: {0}")]
    InvalidDependency(String),
}

#[derive(Debug, Default)]
pub struct DependencyGraph {
    nodes: HashMap<String, PackageNode>,
    edges: HashMap<String, HashSet<String>>,
}

impl DependencyGraph {
    /// Adds a package node to the graph
    pub fn add_node(&mut self, node: PackageNode) -> Result<(), DependencyGraphError> {
        if self.nodes.contains_key(&node.name) {
            return Err(DependencyGraphError::DuplicatePackage(node.name));
        }

        self.nodes.insert(node.name.clone(), node.clone());
        self.edges.insert(node.name, HashSet::new());

        Ok(())
    }

    /// Adds a dependency relationship between two packages
    pub fn add_dependency(
        &mut self,
        package: &str,
        dependency: &str,
    ) -> Result<(), DependencyGraphError> {
        // Verify both packages exist
        if !self.nodes.contains_key(package) {
            return Err(DependencyGraphError::PackageNotFound(package.to_string()));
        }
        if !self.nodes.contains_key(dependency) {
            return Err(DependencyGraphError::PackageNotFound(
                dependency.to_string(),
            ));
        }

        // Add the dependency
        if let Some(deps) = self.edges.get_mut(package) {
            deps.insert(dependency.to_string());
        }

        // Check for cycles after adding the dependency
        if self.has_cycle() {
            // Remove the dependency that caused the cycle
            if let Some(deps) = self.edges.get_mut(package) {
                deps.remove(dependency);
            }
            return Err(DependencyGraphError::CircularDependency(format!(
                "Adding {} as dependency of {} would create a cycle",
                dependency, package
            )));
        }

        Ok(())
    }

    /// Returns true if the graph contains cycles
    pub fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node in self.nodes.keys() {
            if !visited.contains(node) && self.has_cycle_util(node, &mut visited, &mut rec_stack) {
                return true;
            }
        }

        false
    }

    /// Returns a sorted list of packages in installation order
    pub fn installation_order(&self) -> Result<Vec<&PackageNode>, DependencyGraphError> {
        let mut visited = HashSet::new();
        let mut order = Vec::new();

        for node in self.nodes.keys() {
            if !visited.contains(node) {
                self.topological_sort(node, &mut visited, &mut order)?;
            }
        }

        // Claude added this, but the related test fails. Commenting out for now.
        // order.reverse(); // Reverse to get correct installation order
        Ok(order
            .iter()
            .filter_map(|name| self.nodes.get(name))
            .collect())
    }

    /// Returns the number of nodes in the graph
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns true if the graph is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    // Private helper methods

    fn has_cycle_util(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
    ) -> bool {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());

        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    if self.has_cycle_util(dep, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(dep) {
                    return true;
                }
            }
        }

        rec_stack.remove(node);
        false
    }

    fn topological_sort(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        order: &mut Vec<String>,
    ) -> Result<(), DependencyGraphError> {
        visited.insert(node.to_string());

        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    self.topological_sort(dep, visited, order)?;
                }
            }
        }

        order.push(node.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::PackageNodeBuilder;

    fn create_test_package(name: &str) -> PackageNode {
        PackageNodeBuilder::default()
            .name(name)
            .version("1.0.0")
            .environment("test-env", "test install")
            .build()
    }

    #[test]
    fn test_create_empty_graph() {
        let graph = DependencyGraph::default();
        assert!(graph.is_empty());
        assert_eq!(graph.len(), 0);
    }

    #[test]
    fn test_add_single_node() {
        let mut graph = DependencyGraph::default();
        let package = create_test_package("test-package");

        assert!(graph.add_node(package).is_ok());
        assert_eq!(graph.len(), 1);
    }

    #[test]
    fn test_add_duplicate_node() {
        let mut graph = DependencyGraph::default();
        let package1 = create_test_package("test-package");
        let package2 = create_test_package("test-package");

        assert!(graph.add_node(package1).is_ok());
        assert!(matches!(
            graph.add_node(package2),
            Err(DependencyGraphError::DuplicatePackage(_))
        ));
    }

    #[test]
    fn test_add_valid_dependency() {
        let mut graph = DependencyGraph::default();
        let package1 = create_test_package("package1");
        let package2 = create_test_package("package2");

        assert!(graph.add_node(package1).is_ok());
        assert!(graph.add_node(package2).is_ok());
        assert!(graph.add_dependency("package1", "package2").is_ok());
    }

    #[test]
    fn test_add_dependency_missing_package() {
        let mut graph = DependencyGraph::default();
        let package = create_test_package("package1");

        assert!(graph.add_node(package).is_ok());
        assert!(matches!(
            graph.add_dependency("package1", "missing"),
            Err(DependencyGraphError::PackageNotFound(_))
        ));
    }

    #[test]
    fn test_detect_simple_cycle() {
        let mut graph = DependencyGraph::default();
        let package1 = create_test_package("package1");
        let package2 = create_test_package("package2");

        assert!(graph.add_node(package1).is_ok());
        assert!(graph.add_node(package2).is_ok());
        assert!(graph.add_dependency("package1", "package2").is_ok());
        assert!(matches!(
            graph.add_dependency("package2", "package1"),
            Err(DependencyGraphError::CircularDependency(_))
        ));
    }

    #[test]
    fn test_detect_complex_cycle() {
        let mut graph = DependencyGraph::default();
        let package1 = create_test_package("package1");
        let package2 = create_test_package("package2");
        let package3 = create_test_package("package3");

        assert!(graph.add_node(package1).is_ok());
        assert!(graph.add_node(package2).is_ok());
        assert!(graph.add_node(package3).is_ok());
        assert!(graph.add_dependency("package1", "package2").is_ok());
        assert!(graph.add_dependency("package2", "package3").is_ok());
        assert!(matches!(
            graph.add_dependency("package3", "package1"),
            Err(DependencyGraphError::CircularDependency(_))
        ));
    }

    #[test]
    fn test_installation_order() {
        let mut graph = DependencyGraph::default();
        let package1 = create_test_package("package1");
        let package2 = create_test_package("package2");
        let package3 = create_test_package("package3");

        assert!(graph.add_node(package1).is_ok());
        assert!(graph.add_node(package2).is_ok());
        assert!(graph.add_node(package3).is_ok());
        assert!(graph.add_dependency("package1", "package2").is_ok());
        assert!(graph.add_dependency("package2", "package3").is_ok());

        let order = graph.installation_order().unwrap();
        assert_eq!(order.len(), 3);
        dbg!(&order);
        assert_eq!(order[0].name, "package3");
        assert_eq!(order[1].name, "package2");
        assert_eq!(order[2].name, "package1");
    }
}
