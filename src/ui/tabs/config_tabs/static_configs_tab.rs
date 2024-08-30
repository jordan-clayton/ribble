use egui::{Grid, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::VariantArray;
use whisper_realtime::{configs::Configs, model::ModelType};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::tab_view,
    utils::{configs::AudioConfigs, threading::get_max_threads},
};
use crate::ui::tabs::config_tabs::configs_common;
use crate::utils::configs::AudioConfigType;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StaticConfigsTab {
    title: String,
    static_configs: Configs,
    #[serde(skip)]
    #[serde(default = "get_max_threads")]
    max_threads: std::ffi::c_int,
}

impl StaticConfigsTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: Configs) -> Self {
        let max_threads = get_max_threads();
        Self {
            title: String::from("Static"),
            static_configs: configs,
            max_threads,
        }
    }
}

impl Default for StaticConfigsTab {
    fn default() -> Self {
        let configs = Configs::default();
        Self::new_with_configs(configs)
    }
}

impl tab_view::TabView for StaticConfigsTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Main UI design.
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let c_configs = self.static_configs.clone();
        let Self {
            title: _,
            static_configs,
            max_threads,
        } = self;

        let Configs {
            n_threads,
            set_translate,
            language,
            use_gpu,
            model,
            realtime_timeout: _,
            audio_sample_ms: _,
            vad_sample_ms: _,
            phrase_timeout: _,
            voice_probability_threshold: _,
            naive_vad_freq_threshold: _,
            naive_vad_energy_threshold: _,
            naive_window_len: _,
            naive_window_step: _,
            print_special: _,
            print_progress: _,
            print_realtime: _,
            print_timestamps: _,
        } = static_configs;

        // Check for config-copy requests
        let req = controller.recv_static_configs_req();
        if let Ok(_) = req {
            controller
                .send_configs(AudioConfigs::Static(c_configs))
                .expect("Configs channel closed.");
        }

        let static_running = controller.static_running();
        let available_models: Vec<ModelType> = ModelType::VARIANTS.to_vec();

        ui.add_enabled_ui(!static_running, |ui| {
            Grid::new("static_configs").striped(true).show(ui, |ui| {
                // MODEL ROW, see configs common.
                // Contains dropdown to select model type, to open a downloaded model + download a model.
                configs_common::model_row(
                    ui,
                    model,
                    AudioConfigType::Static,
                    controller.clone(),
                    available_models.as_slice(),
                );
                ui.end_row();
                // Num_threads
                configs_common::n_threads_row(ui, n_threads, *max_threads);
                ui.end_row();
                let gpu_enabled = controller.gpu_enabled();
                configs_common::use_gpu_row(ui, use_gpu, gpu_enabled);
                ui.end_row();
                // INPUT Language -> Set to auto for language detection
                configs_common::set_language_row(ui, language);
                ui.end_row();
                // Translate (TO ENGLISH)
                configs_common::set_translate_row(ui, set_translate);
                ui.end_row();
                // Reset defaults button.
                ui.label("Reset To Defaults");
                if ui.button("Reset").clicked() {
                    let default = Configs::default();
                    let Configs {
                        n_threads: default_n_threads,
                        set_translate: default_set_translate,
                        language: default_language,
                        use_gpu: default_use_gpu,
                        model: default_model,
                        realtime_timeout: _,
                        audio_sample_ms: _,
                        vad_sample_ms: _,
                        phrase_timeout: _,
                        voice_probability_threshold: _,
                        naive_vad_freq_threshold: _,
                        naive_vad_energy_threshold: _,
                        naive_window_len: _,
                        naive_window_step: _,
                        print_special: _,
                        print_progress: _,
                        print_realtime: _,
                        print_timestamps: _,
                    } = default;

                    *n_threads = default_n_threads;
                    *set_translate = default_set_translate;
                    *language = default_language;
                    *use_gpu = default_use_gpu;
                    *model = default_model;
                }
                ui.end_row();
            });
        });
    }

    // Right-click tab -> What should be shown.
    // TODO: Determine whether necessary to implement
    fn context_menu(
        &mut self,
        _ui: &mut Ui,
        _controller: &mut WhisperAppController,
        _surface: SurfaceIndex,
        _node: NodeIndex,
    ) {}

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}
