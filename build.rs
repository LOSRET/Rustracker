use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    let has_dashboard = env::var("CARGO_FEATURE_DASHBOARD").is_ok();

    if has_dashboard {
        let frontend_dir = Path::new(&manifest_dir).join("frontend");
        let dist_dir = Path::new(&manifest_dir).join("dist");

        // Build the Vue frontend if package.json exists.
        if frontend_dir.join("package.json").exists() {
            let npm = if cfg!(target_os = "windows") {
                "npm.cmd"
            } else {
                "npm"
            };
            let status = Command::new(npm)
                .args(["run", "build"])
                .current_dir(&frontend_dir)
                .status()
                .expect("failed to run `npm run build` (is Node installed?)");
            if !status.success() {
                panic!("`npm run build` exited with {status}");
            }
        }

        // Copy dist/index.html → OUT_DIR/index.html.
        // Contact info injection is handled by Vite at build time via the
        // VITE_PERSONAL_CONTACT env var, so no post-processing needed here.
        let index_html =
            fs::read_to_string(dist_dir.join("index.html")).expect("dist/index.html not found");

        fs::write(out_path.join("index.html"), &index_html)
            .expect("failed to write OUT_DIR/index.html");

        // Copy dist/assets/* → OUT_DIR/assets/* and generate a manifest of embedded files.
        let src_assets = dist_dir.join("assets");
        let dst_assets = out_path.join("assets");
        let _ = fs::remove_dir_all(&dst_assets);
        fs::create_dir_all(&dst_assets).expect("failed to create OUT_DIR/assets");

        let mut entries: Vec<String> = Vec::new();
        if src_assets.exists() {
            for entry in fs::read_dir(&src_assets).expect("failed to read dist/assets") {
                let entry = entry.expect("failed to read dir entry");
                let name = entry.file_name().to_string_lossy().to_string();
                fs::copy(entry.path(), dst_assets.join(&name))
                    .expect("failed to copy asset to OUT_DIR/assets");
                entries.push(name);
            }
        }
        entries.sort();

        let mut manifest = String::from("pub(crate) static ASSETS: &[(&str, &[u8])] = &[\n");
        for name in &entries {
            manifest.push_str(&format!(
                "    ({name:?}, include_bytes!(concat!(env!(\"OUT_DIR\"), \"/assets/\", {name:?}))),\n"
            ));
        }
        manifest.push_str("];\n");
        fs::write(out_path.join("assets_manifest.rs"), &manifest)
            .expect("failed to write assets_manifest.rs");

        println!(
            "cargo:rerun-if-changed={}",
            frontend_dir.join("package.json").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            frontend_dir.join("vite.config.ts").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            frontend_dir.join("index.html").display()
        );
        for name in &entries {
            println!("cargo:rerun-if-changed={}", src_assets.join(name).display());
        }
    }

    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_DASHBOARD");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_PERSONAL_CONTACT");
}
