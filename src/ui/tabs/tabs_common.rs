use std::path::PathBuf;

use egui::{Response, Ui, Widget, WidgetInfo, WidgetText, WidgetType};
use rfd::FileDialog;

pub enum FileDialogAction {
    NoAction,
    PickSingleFile(Box<dyn Fn(&Option<PathBuf>)>),
    PickMultipleFiles(Box<dyn Fn(&Option<Vec<PathBuf>>)>),
    PickSingleFolder(Box<dyn Fn(&Option<PathBuf>)>),
    PickMultipleFolders(Box<dyn Fn(&Option<Vec<PathBuf>>)>),
    SaveFile(Box<dyn Fn(&Option<PathBuf>)>),
}

#[derive(Clone)]
pub struct FileFilter<T: Into<String> + Clone, S: ToString + Clone> {
    pub file_type: T,
    pub filters: Vec<S>,
}

impl<T: Into<String> + Clone, S: ToString + Clone> FileFilter<T, S> {
    fn unpack(self) -> (T, Vec<S>) {
        (self.file_type, self.filters)
    }
}

impl<T: Into<String> + Clone, S: ToString + Clone> Into<(T, Vec<S>)> for FileFilter<T, S> {
    fn into(self) -> (T, Vec<S>) {
        self.unpack()
    }
}

// Generic draw "action button"

fn draw_action_button<
    T: Into<WidgetText> + Clone,
    L: Into<WidgetText> + Clone,
    SL: ToString + Clone,
    CB: FnOnce(),
>(
    ui: &mut Ui,
    button_label: L,
    semantic_label: SL,
    tooltip: Option<T>,
    tooltip_selectable: bool,
    clicked_callback: CB,
) -> Response {
    let mut button = ui.button(button_label.clone());

    if let Some(t) = tooltip {
        button = button.on_hover_ui(|ui| {
            ui.style_mut().interaction.selectable_labels = tooltip_selectable;
            ui.label(t);
        });
    };

    if button.clicked() {
        clicked_callback();
        button.mark_changed();
    }

    button.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, semantic_label.clone()));
    button
}

// Generic draw "file dialog button"
fn draw_file_dialog_button<
    T: Into<String> + Clone,
    S: ToString + Clone,
    L: Into<WidgetText> + Clone,
    BL: Into<WidgetText> + Clone,
    SL: ToString + Clone,
>(
    ui: &mut Ui,
    button_label: BL,
    semantic_label: SL,
    filters: Option<&[FileFilter<T, S>]>,
    tooltip: Option<L>,
    tooltip_selectable: bool,
    file_action: FileDialogAction,
) -> Response {
    let clicked_callback = move || {
        let mut file_dialog = FileDialog::new();

        // Unpack filters
        if let Some(f) = filters {
            for filter in f {
                let (file_type, file_filters) = filter.clone().into();
                file_dialog = file_dialog.add_filter(file_type, &file_filters);
            }
        }

        // Open File dialog at HOME directory, fallback to root.
        let base_dirs = directories::BaseDirs::new();
        let dir = if let Some(dir) = base_dirs {
            dir.home_dir().to_path_buf()
        } else {
            PathBuf::from("/")
        };

        file_dialog = file_dialog.set_directory(&dir);

        match file_action {
            FileDialogAction::NoAction => {}
            FileDialogAction::PickSingleFile(f) => {
                let fp = file_dialog.pick_file();
                f(&fp);
            }
            FileDialogAction::PickMultipleFiles(f) => {
                let fp = file_dialog.pick_files();
                f(&fp);
            }
            FileDialogAction::PickSingleFolder(f) => {
                let fp = file_dialog.pick_folder();
                f(&fp);
            }
            FileDialogAction::PickMultipleFolders(f) => {
                let fp = file_dialog.pick_folders();
                f(&fp);
            }
            FileDialogAction::SaveFile(f) => {
                let fp = file_dialog.save_file();
                f(&fp);
            }
        }
    };
    draw_action_button(
        ui,
        button_label,
        semantic_label,
        tooltip,
        tooltip_selectable,
        clicked_callback,
    )
}

// OPEN FILE
fn draw_open_file_button<
    T: Into<String> + Clone,
    L: Into<WidgetText> + Clone,
    S: ToString + Clone,
    CB: Fn(&Option<PathBuf>) + 'static,
>(
    ui: &mut Ui,
    filters: Option<&[FileFilter<T, S>]>,
    tooltip: Option<L>,
    tooltip_selectable: bool,
    handle_file_open: Box<CB>,
) -> Response {
    draw_file_dialog_button(
        ui,
        "Open",
        "Open File",
        filters,
        tooltip,
        tooltip_selectable,
        FileDialogAction::PickSingleFile(handle_file_open),
    )
}

pub fn open_file_button<
    T: Into<String> + Clone + 'static,
    L: Into<WidgetText> + Clone + 'static,
    S: ToString + Clone + 'static,
    CB: Fn(&Option<PathBuf>) + 'static,
>(
    filters: Option<&[FileFilter<T, S>]>,
    tooltip: Option<L>,
    tooltip_selectable: bool,
    handle_file_open: CB,
) -> impl Widget + '_ {
    move |ui: &mut Ui| {
        draw_open_file_button(
            ui,
            filters,
            tooltip,
            tooltip_selectable,
            Box::new(handle_file_open),
        )
    }
}

// SAVE FILE
fn draw_save_file_button<
    T: Into<String> + Clone,
    L: Into<WidgetText> + Clone,
    S: ToString + Clone,
    CB: Fn(&Option<PathBuf>) + 'static,
>(
    ui: &mut Ui,
    filters: Option<&[FileFilter<T, S>]>,
    tooltip: Option<L>,
    tooltip_selectable: bool,
    handle_save: Box<CB>,
) -> Response {
    draw_file_dialog_button(
        ui,
        "Save",
        "Save File",
        filters,
        tooltip,
        tooltip_selectable,
        FileDialogAction::SaveFile(handle_save),
    )
}

pub fn safe_file_button<
    T: Into<String> + Clone + 'static,
    L: Into<WidgetText> + Clone + 'static,
    S: ToString + Clone + 'static,
    CB: Fn(&Option<PathBuf>) + 'static,
>(
    filters: Option<&[FileFilter<T, S>]>,
    tooltip: Option<L>,
    tooltip_selectable: bool,
    handle_save: CB,
) -> impl Widget + '_ {
    move |ui: &mut Ui| {
        draw_save_file_button(
            ui,
            filters,
            tooltip,
            tooltip_selectable,
            Box::new(handle_save),
        )
    }
}

// Download button
fn draw_download_button<L: Into<WidgetText> + Clone, SL: ToString + Clone, CB: FnOnce()>(
    ui: &mut Ui,
    tooltip: Option<L>,
    tooltip_selectable: bool,
    semantic_label: SL,
    download: CB,
) -> Response {
    draw_action_button(
        ui,
        "Download",
        semantic_label,
        tooltip,
        tooltip_selectable,
        download,
    )
}

pub fn download_button<
    T: Into<WidgetText> + Clone + 'static,
    SL: ToString + Clone + 'static,
    CB: FnOnce() + 'static,
>(
    tooltip: Option<T>,
    tooltip_selectable: bool,
    semantic_label: SL,
    download: CB,
) -> impl Widget {
    move |ui: &mut Ui| {
        draw_download_button(ui, tooltip, tooltip_selectable, semantic_label, download)
    }
}
