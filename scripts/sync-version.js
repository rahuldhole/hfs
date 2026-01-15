import { readFileSync, writeFileSync } from 'fs';
import { join } from 'path';

// Paths
const pkgPath = join(process.cwd(), 'package.json');
const tauriConfPath = join(process.cwd(), 'src-tauri', 'tauri.conf.json');
const cargoPath = join(process.cwd(), 'src-tauri', 'Cargo.toml');

// Read Package.json
const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8'));
const version = pkg.version;

console.log(`Syncing version ${version} to Tauri files...`);

// 1. Update tauri.conf.json
try {
    const tauriConf = JSON.parse(readFileSync(tauriConfPath, 'utf-8'));
    tauriConf.version = version;
    writeFileSync(tauriConfPath, JSON.stringify(tauriConf, null, 2));
    console.log('✅ Updated src-tauri/tauri.conf.json');
} catch (error) {
    console.error('❌ Failed to update tauri.conf.json:', error);
}

// 2. Update Cargo.toml
try {
    let cargoToml = readFileSync(cargoPath, 'utf-8');
    // Replace version = "x.y.z" under [package]
    // Regex looks for the specific version line in the package section
    cargoToml = cargoToml.replace(
        /^version = "[^"]+"$/m,
        `version = "${version}"`
    );
    writeFileSync(cargoPath, cargoToml, 'utf-8');
    console.log('✅ Updated src-tauri/Cargo.toml');
} catch (error) {
    console.error('❌ Failed to update Cargo.toml:', error);
}

console.log('Version sync complete.');
