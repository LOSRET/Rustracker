use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let index_path = Path::new(&manifest_dir).join("assets/index.html");
    let contact_path = Path::new(&manifest_dir).join("assets/contact.html");

    let mut index_html = fs::read_to_string(&index_path).expect("failed to read assets/index.html");

    let has_contact = env::var("CARGO_FEATURE_PERSONAL_CONTACT").is_ok();

    if has_contact {
        let contact_html =
            fs::read_to_string(&contact_path).expect("failed to read assets/contact.html");
        index_html = index_html.replace("<!-- CONTACT -->", &contact_html);
    } else {
        index_html = index_html.replace("<!-- CONTACT -->\n", "");
    }

    let out_path = Path::new(&out_dir).join("index.html");
    fs::write(&out_path, &index_html).expect("failed to write generated index.html");

    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_PERSONAL_CONTACT");
    println!("cargo:rerun-if-changed=assets/index.html");
    println!("cargo:rerun-if-changed=assets/contact.html");
}
