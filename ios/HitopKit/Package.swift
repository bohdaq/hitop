// swift-tools-version:6.0
// HitopKit: the hitop collection format v1 (spec/collection-format.md), its
// variables, secrets, scripts and runner, for the iOS companion app. Builds
// and tests on Linux too (`swift test`); MIT like the rest of hitop.
import PackageDescription

let package = Package(
    name: "HitopKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "HitopKit", targets: ["HitopKit"]),
    ],
    dependencies: [
        // MIT. Pinned exactly; Package.resolved is committed.
        .package(url: "https://github.com/jpsim/Yams.git", exact: "6.2.2"),
    ],
    targets: [
        .target(
            name: "HitopKit",
            dependencies: [.product(name: "Yams", package: "Yams")],
            resources: [.copy("Resources/pm-shim.js")]
        ),
        .testTarget(
            name: "HitopKitTests",
            dependencies: ["HitopKit"],
            // Started by path from the tests, not bundled.
            exclude: ["mock_server.py"]
        ),
    ]
)
