fn main() -> eframe::Result {
    eframe::run_native("RExcel", eframe::NativeOptions::default(), Box::new(|_cc| Ok(Box::new(A))))
}
struct A;
impl eframe::App for A {
    fn update(&mut self, ctx: &egui::Context, _f: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| ui.label("hi"));
    }
}
