// examples/progress_display_example.rs
//
// This example demonstrates how to use the progress display components
// in a real application. It simulates installing a package with dependencies
// and shows how to update progress bars and display status information.

use selfie::progress_display::{ProgressManager, ProgressStyleType};
use std::thread;
use std::time::Duration;

fn main() {
    println!("Selfie Package Manager - Progress Display Example");
    println!("=================================================\n");

    // Create a progress manager with colors and Unicode support
    let progress_manager = ProgressManager::new(true, true, true);

    // Create a main progress display for the overall operation
    let main_pb = progress_manager.create_progress_bar(
        "main",
        "Installing package 'example-pkg' with dependencies",
        ProgressStyleType::Message,
    );

    // Step 1: Resolving dependencies
    main_pb.set_message("Resolving dependencies...");
    thread::sleep(Duration::from_millis(800));

    main_pb.set_message("Found 3 packages to install (including 2 dependencies)");
    thread::sleep(Duration::from_millis(500));

    // Step 2: Install Dependency 1
    let dep1_pb = progress_manager.create_progress_bar(
        "dep1",
        "Installing dependency 'dep1'",
        ProgressStyleType::Spinner,
    );

    // Check if already installed
    dep1_pb.set_message("Checking if already installed...");
    thread::sleep(Duration::from_millis(500));

    // Not installed, so we'll install it
    dep1_pb.set_message("Not installed, proceeding with installation...");
    thread::sleep(Duration::from_millis(300));

    // Show progress on a bar
    let dep1_install_pb = progress_manager.create_progress_bar(
        "dep1-install",
        "Downloading and installing dep1...",
        ProgressStyleType::Bar,
    );

    // Simulate installation progress
    for i in 0..=100 {
        dep1_install_pb.set_position(i);

        if i % 20 == 0 {
            // Add some output occasionally if we're in verbose mode
            progress_manager
                .add_progress_line("dep1-install", &format!("Processed {} of 100 steps", i))
                .unwrap();
        }

        thread::sleep(Duration::from_millis(20));
    }

    // Complete the installation
    dep1_install_pb.finish_with_message("Downloaded and installed successfully");
    dep1_pb.finish_with_message("Installation complete (2.5s)");

    // Step 3: Install Dependency 2
    let dep2_pb = progress_manager.create_progress_bar(
        "dep2",
        "Installing dependency 'dep2'",
        ProgressStyleType::Spinner,
    );

    // Check if already installed
    dep2_pb.set_message("Checking if already installed...");
    thread::sleep(Duration::from_millis(500));

    // This one is already installed
    dep2_pb.finish_with_message("Already installed (0.5s)");

    // Step 4: Install main package
    let pkg_pb = progress_manager.create_progress_bar(
        "pkg",
        "Installing 'example-pkg'",
        ProgressStyleType::Spinner,
    );

    // Check if already installed
    pkg_pb.set_message("Checking if already installed...");
    thread::sleep(Duration::from_millis(500));

    // Not installed, so we'll install it
    pkg_pb.set_message("Not installed, proceeding with installation...");
    thread::sleep(Duration::from_millis(300));

    // Installing
    pkg_pb.set_message("Installing...");

    // Show some command output
    progress_manager
        .add_command_output("pkg", "stdout", "Downloading example-pkg v1.0.0...")
        .unwrap();
    thread::sleep(Duration::from_millis(800));

    progress_manager
        .add_command_output("pkg", "stdout", "Extracting archive...")
        .unwrap();
    thread::sleep(Duration::from_millis(500));

    progress_manager
        .add_command_output("pkg", "stdout", "Building from source...")
        .unwrap();
    thread::sleep(Duration::from_millis(1200));

    progress_manager
        .add_command_output("pkg", "stdout", "Running post-install hooks...")
        .unwrap();
    thread::sleep(Duration::from_millis(600));

    // Complete the installation
    pkg_pb.finish_with_message("Installation complete (3.1s)");

    // Show summary
    let summary_pb = progress_manager.create_progress_bar(
        "summary",
        "Installation Summary",
        ProgressStyleType::Message,
    );

    // Display summary information
    summary_pb.println("Total time: 7.2s");
    summary_pb.println("Dependencies: 3.0s");
    summary_pb.println("Package: 3.1s");

    // Display success message
    summary_pb.finish_with_message("Successfully installed 'example-pkg' and 2 dependencies");

    println!("\nExample completed!");
}
