// swift-tools-version:5.3
import PackageDescription
import Foundation
let package = Package(
    name: "MetaSecretCore",
    platforms: [
        .iOS(.v13),
    ],
    targets: [
        .systemLibrary(name: "metasecretcore"),
        .target(name: "MetaSecretCoreLib", dependencies: ["metasecretcore"])
    ]
)