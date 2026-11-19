//! Tutorial steps ship with every wizard template.

use seal_editor_core::{
    apply_template, create_project, load_guide, project_tutorials, CreateProjectOptions,
    PROJECT_TEMPLATES,
};
use std::fs;

#[test]
fn each_template_writes_a_tutorial_guide() {
    for template in PROJECT_TEMPLATES {
        let dir = std::env::temp_dir().join(format!(
            "seal-tut-all-{}-{}",
            template.id,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        create_project(&dir, "Guide").unwrap();
        apply_template(&dir, "Guide", template.id, &CreateProjectOptions::default()).unwrap();
        let guide = load_guide(&dir).unwrap().expect(template.id);
        assert_eq!(guide.template_id, template.id);
        assert!(!guide.steps.is_empty(), "{}", template.id);
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn damaged_progress_file_keeps_done_empty() {
    let dir = std::env::temp_dir().join(format!("seal-tut-bad-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    create_project(&dir, "Bad").unwrap();
    project_tutorials::write_shipped(&dir, "2d").unwrap();
    std::fs::write(
        dir.join("tutorial/progress.sealtutorial"),
        "schema_version = 1\ndone = [broken\n",
    )
    .unwrap();
    assert!(project_tutorials::load_done(&dir).unwrap().is_empty());
    let _ = fs::remove_dir_all(&dir);
}