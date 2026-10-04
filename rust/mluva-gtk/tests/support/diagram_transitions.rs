//! Actual ephemeral WebKit edits, texture reuse, untrusted content and renderer loss.
use super::settle;
use adw::prelude::*;
use mluva_gtk::{
    document_layout::DocumentResources, markdown_view::MarkdownTextView, mermaid::MermaidPreview,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

fn until(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(15);
    while !ready() {
        assert!(Instant::now() < end, "diagram transition did not settle");
        settle();
    }
}
fn children(widget: &gtk::Box) -> Vec<gtk::Widget> {
    let mut children = vec![];
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        children.push(current);
    }
    children
}
fn pictures(preview: &MermaidPreview) -> Vec<gtk::gdk::Texture> {
    children(&preview.widget)
        .iter()
        .filter_map(|w| {
            w.downcast_ref::<gtk::Picture>()?
                .paintable()?
                .downcast::<gtk::gdk::Texture>()
                .ok()
        })
        .collect()
}
pub fn exercise(body: &gtk::Box, resources: &DocumentResources, root: &Path) {
    let reference: Value = serde_json::from_str(include_str!(
        "../fixtures/released-diagram-transitions.json"
    ))
    .unwrap();
    assert_eq!(
        reference["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    assert_eq!(
        reference["gtk"],
        json!([
            gtk::major_version(),
            gtk::minor_version(),
            gtk::micro_version()
        ])
    );
    assert_eq!(reference["pango"], gtk::pango::version_string().as_str());
    let editor = MarkdownTextView::document(
        reference["stages"][0]["source"].as_str().unwrap(),
        true,
        true,
    );
    let preview = MermaidPreview::new(&editor, resources);
    body.append(&editor);
    body.append(&preview.widget);
    until(|| !pictures(&preview).is_empty());
    let web = children(&preview.widget)
        .into_iter()
        .find(|w| w.type_().name() == "WebKitWebView")
        .unwrap();
    let session = web.property::<glib::Object>("network-session");
    let ephemeral = session.property::<bool>("is-ephemeral");
    let requests = Rc::new(RefCell::new(Vec::<String>::new()));
    let received = requests.clone();
    web.connect_local("resource-load-started", false, move |values| {
        let request = values[2].get::<glib::Object>().unwrap();
        let uri = request.property::<String>("uri");
        received.borrow_mut().push(uri);
        None
    });
    let rendered = Rc::new(Cell::new(0));
    let received = rendered.clone();
    web.property::<glib::Object>("user-content-manager")
        .connect_local("script-message-received::rendered", false, move |_| {
            received.set(received.get() + 1);
            None
        });
    let original = pictures(&preview)[0].clone();
    for stage in reference["stages"].as_array().unwrap() {
        let name = stage["name"].as_str().unwrap();
        let before = rendered.get();
        match name {
            "sequence" => {}
            "renderer-terminated" => {
                // Invoke WebKit's public process-termination API, the same actual
                // boundary used by the renderer watchdog, without a production seam.
                unsafe {
                    let library = libloading::Library::new("libwebkitgtk-6.0.so.4").unwrap();
                    let terminate: libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void)> =
                        library
                            .get(c"webkit_web_view_terminate_web_process".to_bytes_with_nul())
                            .unwrap();
                    terminate(web.as_ptr().cast());
                }
                until(|| {
                    !children(&preview.widget)
                        .iter()
                        .any(|w| w.type_().name() == "WebKitWebView")
                });
            }
            "paragraph-keeps-render" => {
                editor.buffer().set_text(stage["source"].as_str().unwrap());
                let end = Instant::now() + Duration::from_millis(750);
                while Instant::now() < end {
                    settle();
                }
                assert_eq!(
                    rendered.get(),
                    before,
                    "unchanged diagram must not start a new render"
                );
            }
            "unfinished" => {
                editor.buffer().set_text(stage["source"].as_str().unwrap());
                until(|| !preview.widget.get_visible());
            }
            _ => {
                editor.buffer().set_text(stage["source"].as_str().unwrap());
                until(|| rendered.get() > before);
            }
        }
        let textures=pictures(&preview).iter().enumerate().map(|(index,texture)|{
            let bytes=texture.save_to_png_bytes();fs::write(root.join(format!("transition-{name}-{index}.png")),&bytes).unwrap();
            json!({"width":texture.width(),"height":texture.height(),"sha256":glib::compute_checksum_for_data(glib::ChecksumType::Sha256,&bytes).unwrap().as_str()})
        }).collect::<Vec<_>>();
        let buffer = editor.buffer();
        let mut value = json!({"name":name,"source":editor.text(),"visible":buffer.text(&buffer.start_iter(),&buffer.end_iter(),false).as_str(),"textures":textures,"notice":preview.notice.label().as_str(),"notice_visible":preview.notice.get_visible(),"preview_visible":preview.widget.get_visible(),"renderer_present":children(&preview.widget).iter().any(|w|w.type_().name()=="WebKitWebView"),"external_requests":requests.borrow().iter().filter(|uri|uri.starts_with("http:")||uri.starts_with("https:")).collect::<Vec<_>>()});
        if name == "sequence" {
            value["ephemeral"] = json!(ephemeral);
        }
        if name == "paragraph-keeps-render" {
            value["texture_reused"] = json!(pictures(&preview)[0] == original);
        }
        fs::write(
            root.join(format!("diagram-{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        assert_eq!(&value, stage, "diagram transition {name}");
        println!("DIAGRAM_TRANSITION {name} PASS");
    }
    drop(web);
    body.remove(&preview.widget);
    body.remove(&editor);
    drop(preview);
    drop(editor);
    let end = Instant::now() + Duration::from_millis(500);
    while Instant::now() < end {
        settle();
    }
}
