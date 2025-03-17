// src/domain/dependency.rs
// Dependency graph and related types

use std::collections::{HashMap, HashSet};
use thiserror::Error;

use super::package::Package;

/// Errors that can occur during dependency operations
#[derive(Debug, Error)]
pub(crate) enum DependencyGraphError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String, Vec<String>),

    #[error("Invalid dependency: {0}")]
    InvalidDependency(String),
}

/// Represents a graph of package dependencies
#[derive(Debug, Default)]
pub(crate) struct DependencyGraph {
    /// Map of package name to package
    nodes: HashMap<String, Package>,

    /// Map of package name to its dependencies
    edges: HashMap<String, HashSet<String>>,
}

impl DependencyGraph {
    /// Add a package node to the graph
    pub(crate) fn add_node(&mut self, package: Package) -> Result<(), DependencyGraphError> {
        let name = package.name.clone();

        // If the node already exists, update it
        self.nodes.insert(name.clone(), package);

        // Ensure the package has an entry in the edges map
        self.edges.entry(name).or_default();

        Ok(())
    }

    /// Add a dependency relationship between packages
    pub(crate) fn add_dependency(
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

            // Create a simple path for the cycle
            let path = vec![package.to_string(), dependency.to_string()];

            return Err(DependencyGraphError::CircularDependency(
                format!(
                    "Adding {} as dependency of {} would create a cycle",
                    dependency, package
                ),
                path,
            ));
        }

        Ok(())
    }

    /// Check if the graph contains any cycles
    pub(crate) fn has_cycle(&self) -> bool {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        for node in self.nodes.keys() {
            if !visited.contains(node) && self.has_cycle_util(node, &mut visited, &mut rec_stack) {
                return true;
            }
        }

        false
    }

    /// Get a list of packages in installation order (dependencies first)
    pub(crate) fn installation_order(&self) -> Result<Vec<&Package>, DependencyGraphError> {
        // Use topological sort to get installation order
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();

        // Start DFS from each unvisited node
        for node_name in self.nodes.keys() {
            if !visited.contains(node_name) {
                self.topological_sort_util(
                    node_name,
                    &mut visited,
                    &mut temp_visited,
                    &mut result,
                )?;
            }
        }

        Ok(result)
    }

    /// Get the number of packages in the graph
    #[allow(dead_code)]
    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the graph is empty
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Get a list of all package names in the graph
    pub(crate) fn get_package_names(&self) -> Vec<String> {
        self.nodes.keys().cloned().collect()
    }

    // Private helper methods

    /// Detect cycles using DFS
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

    /// Find all cycles in the graph
    pub(crate) fn find_cycles(&self) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut path = Vec::new();
        let mut on_path = HashSet::new();

        // Start DFS from each node
        for node in self.nodes.keys() {
            if !visited.contains(node) {
                self.find_cycles_util(node, &mut visited, &mut path, &mut on_path, &mut cycles);
            }
        }

        cycles
    }

    /// Helper for find_cycles that performs the DFS
    fn find_cycles_util(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        on_path: &mut HashSet<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        // If already on current path, we found a cycle
        if on_path.contains(node) {
            // Find the start of the cycle
            let cycle_start = path.iter().position(|n| n == node).unwrap();

            // Extract the cycle path
            let mut cycle = path[cycle_start..].to_vec();
            cycle.push(node.to_string()); // Complete the cycle
            cycles.push(cycle);
            return;
        }

        // Mark as visited and on current path
        visited.insert(node.to_string());
        on_path.insert(node.to_string());
        path.push(node.to_string());

        // Visit all adjacent nodes
        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    self.find_cycles_util(dep, visited, path, on_path, cycles);
                } else if on_path.contains(dep) {
                    // Cycle with an already visited node on current path
                    let cycle_start = path.iter().position(|n| n == dep).unwrap();
                    let mut cycle = path[cycle_start..].to_vec();
                    cycle.push(dep.to_string());
                    cycles.push(cycle);
                }
            }
        }

        // Remove from current path when done with this node
        on_path.remove(node);
        path.pop();
    }

    /// Perform topological sort to order packages
    fn topological_sort_util<'a>(
        &'a self,
        node: &str,
        visited: &mut HashSet<String>,
        temp_visited: &mut HashSet<String>,
        result: &mut Vec<&'a Package>,
    ) -> Result<(), DependencyGraphError> {
        // Check for cycle using temporary visit mark
        if temp_visited.contains(node) {
            // Find the cycle path for better error reporting
            let mut cycle_path = Vec::new();
            for n in temp_visited.iter() {
                cycle_path.push(n.clone());
            }
            cycle_path.push(node.to_string());

            return Err(DependencyGraphError::CircularDependency(
                format!("Circular dependency detected involving {}", node),
                cycle_path,
            ));
        }

        // Skip if already visited
        if visited.contains(node) {
            return Ok(());
        }

        // Mark as temporarily visited
        temp_visited.insert(node.to_string());

        // Process all dependencies first
        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                self.topological_sort_util(dep, visited, temp_visited, result)?;
            }
        }

        // Mark as permanently visited
        visited.insert(node.to_string());

        // Add to result after dependencies
        result.push(self.nodes.get(node).unwrap());

        // Remove from temp visited
        temp_visited.remove(node);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::package::PackageBuilder;

    fn create_test_package(name: &str) -> Package {
        PackageBuilder::default()
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
        assert!(graph.get_package_names().is_empty());
    }

    #[test]
    fn test_add_single_node() {
        let mut graph = DependencyGraph::default();
        let package = create_test_package("test-package");

        assert!(graph.add_node(package).is_ok());
        assert_eq!(graph.len(), 1);
        assert_eq!(graph.get_package_names(), vec!["test-package"]);
    }

    #[test]
    fn test_update_existing_node() {
        let mut graph = DependencyGraph::default();
        let package1 = create_test_package("test-package");

        // Create a slightly different version
        let package2 = PackageBuilder::default()
            .name("test-package")
            .version("1.1.0") // Different version
            .environment("test-env", "test install")
            .build();

        assert!(graph.add_node(package1).is_ok());
        assert!(graph.add_node(package2).is_ok()); // Should succeed
        assert_eq!(graph.len(), 1); // Still only 1 node

        // The node should have been updated to the new version
        let nodes = graph.installation_order().unwrap();
        assert_eq!(nodes[0].version, "1.1.0");
    }

    #[test]
    fn test_add_valid_dependency() {
        let mut graph = DependencyGraph::default();
        let package1 = create_test_package("package1");
        let package2 = create_test_package("package2");

        assert!(graph.add_node(package1).is_ok());
        assert!(graph.add_node(package2).is_ok());
        assert!(graph.add_dependency("package1", "package2").is_ok());

        // Check that dependency is reflected in installation order
        let order = graph.installation_order().unwrap();
        assert_eq!(order.len(), 2);
        assert_eq!(order[0].name, "package2"); // Dependency comes first
        assert_eq!(order[1].name, "package1");
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
            Err(DependencyGraphError::CircularDependency(_, _))
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
            Err(DependencyGraphError::CircularDependency(_, _))
        ));
    }

    #[test]
    fn test_installation_order_simple() {
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
        assert_eq!(order[0].name, "package3"); // Deepest dependency first
        assert_eq!(order[1].name, "package2");
        assert_eq!(order[2].name, "package1");
    }

    #[test]
    fn test_installation_order_diamond() {
        let mut graph = DependencyGraph::default();

        // Create a diamond dependency: main -> (dep1, dep2) -> common
        let main = create_test_package("main");
        let dep1 = create_test_package("dep1");
        let dep2 = create_test_package("dep2");
        let common = create_test_package("common");

        assert!(graph.add_node(main).is_ok());
        assert!(graph.add_node(dep1).is_ok());
        assert!(graph.add_node(dep2).is_ok());
        assert!(graph.add_node(common).is_ok());

        assert!(graph.add_dependency("main", "dep1").is_ok());
        assert!(graph.add_dependency("main", "dep2").is_ok());
        assert!(graph.add_dependency("dep1", "common").is_ok());
        assert!(graph.add_dependency("dep2", "common").is_ok());

        let order = graph.installation_order().unwrap();
        assert_eq!(order.len(), 4);

        // Common must come first, then dep1 and dep2 (order between them doesn't matter), then main
        assert_eq!(order[0].name, "common");
        assert!(order[1].name == "dep1" || order[1].name == "dep2");
        assert!(order[2].name == "dep1" || order[2].name == "dep2");
        assert_ne!(order[1].name, order[2].name); // dep1 and dep2 should be different
        assert_eq!(order[3].name, "main");
    }

    #[test]
    fn test_installation_order_multiple_deps() {
        let mut graph = DependencyGraph::default();

        // Create a package with multiple direct dependencies
        let main = create_test_package("main");
        let dep1 = create_test_package("dep1");
        let dep2 = create_test_package("dep2");
        let dep3 = create_test_package("dep3");

        assert!(graph.add_node(main).is_ok());
        assert!(graph.add_node(dep1).is_ok());
        assert!(graph.add_node(dep2).is_ok());
        assert!(graph.add_node(dep3).is_ok());

        assert!(graph.add_dependency("main", "dep1").is_ok());
        assert!(graph.add_dependency("main", "dep2").is_ok());
        assert!(graph.add_dependency("main", "dep3").is_ok());

        let order = graph.installation_order().unwrap();
        assert_eq!(order.len(), 4);

        // Dependencies can be in any order, but main must be last
        assert!(order[0].name == "dep1" || order[0].name == "dep2" || order[0].name == "dep3");
        assert!(order[1].name == "dep1" || order[1].name == "dep2" || order[1].name == "dep3");
        assert!(order[2].name == "dep1" || order[2].name == "dep2" || order[2].name == "dep3");
        assert_eq!(order[3].name, "main");

        // All dependencies must be different
        assert_ne!(order[0].name, order[1].name);
        assert_ne!(order[0].name, order[2].name);
        assert_ne!(order[1].name, order[2].name);
    }

    #[test]
    fn test_complex_dependency_graph() {
        let mut graph = DependencyGraph::default();

        // Create a more complex graph:
        // A -> B -> D
        // A -> C -> D
        // A -> E
        // Where D is a shared dependency of B and C

        let a = create_test_package("A");
        let b = create_test_package("B");
        let c = create_test_package("C");
        let d = create_test_package("D");
        let e = create_test_package("E");

        assert!(graph.add_node(a).is_ok());
        assert!(graph.add_node(b).is_ok());
        assert!(graph.add_node(c).is_ok());
        assert!(graph.add_node(d).is_ok());
        assert!(graph.add_node(e).is_ok());

        assert!(graph.add_dependency("A", "B").is_ok());
        assert!(graph.add_dependency("A", "C").is_ok());
        assert!(graph.add_dependency("A", "E").is_ok());
        assert!(graph.add_dependency("B", "D").is_ok());
        assert!(graph.add_dependency("C", "D").is_ok());

        let order = graph.installation_order().unwrap();
        assert_eq!(order.len(), 5);

        // Verify the topological ordering constraints
        let a_pos = order.iter().position(|p| p.name == "A").unwrap();
        let b_pos = order.iter().position(|p| p.name == "B").unwrap();
        let c_pos = order.iter().position(|p| p.name == "C").unwrap();
        let d_pos = order.iter().position(|p| p.name == "D").unwrap();
        let e_pos = order.iter().position(|p| p.name == "E").unwrap();

        // A must come after B, C, and E
        assert!(a_pos > b_pos);
        assert!(a_pos > c_pos);
        assert!(a_pos > e_pos);

        // B and C must come after D
        assert!(b_pos > d_pos);
        assert!(c_pos > d_pos);
    }
}
