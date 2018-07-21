Write-Host "Syncing Asset Folder..."
robocopy assets target/debug/assets /mir /xo
cargo run --features bedrock/VK_EXT_debug_report,bedrock/VK_KHR_win32_surface
