use crate::GuiApp;
use crate::state::{
    FrequencyUnit, PeriodUnit, TimeUnit, WorkspaceTimingTab,
};


impl GuiApp {
    pub(crate) fn time_settings_from_selection(
            tab: WorkspaceTimingTab,
            frequency_unit: FrequencyUnit,
            period_unit: PeriodUnit,
        ) -> (TimeUnit, f64, String) {
            let unit = match tab {
                WorkspaceTimingTab::Period => match period_unit {
                    PeriodUnit::Ns => TimeUnit::Ns,
                    PeriodUnit::Us => TimeUnit::Us,
                    PeriodUnit::Ms => TimeUnit::Ms,
                    PeriodUnit::S => TimeUnit::S,
                },
                WorkspaceTimingTab::Frequency => match frequency_unit {
                    FrequencyUnit::Hz => TimeUnit::S,
                    FrequencyUnit::KHz => TimeUnit::Ms,
                    FrequencyUnit::MHz => TimeUnit::Us,
                },
            };
            let (scale, label) = match unit {
                TimeUnit::Ns => (1e9, "time_ns"),
                TimeUnit::Us => (1e6, "time_us"),
                TimeUnit::Ms => (1e3, "time_ms"),
                TimeUnit::S => (1.0, "time_s"),
            };
            (unit, scale, label.to_string())
        }

    pub(crate) fn compute_period_seconds(&self) -> f64 {
            self.period_seconds_from_fields()
        }

    pub(crate) fn period_seconds_from_fields(&self) -> f64 {
            match self.period_unit {
                PeriodUnit::Ns => self.period_value * 1e-9,
                PeriodUnit::Us => self.period_value * 1e-6,
                PeriodUnit::Ms => self.period_value * 1e-3,
                PeriodUnit::S => self.period_value,
            }
        }

}