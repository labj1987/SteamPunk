use crate::applog;
use crate::launcher::{self, LaunchTarget};
use crate::library::{self, Trainer};
use crate::setup;

use gtk4::prelude::*;
use gtk4::{
    Align, Box as GtkBox, Button, DropTarget, FileDialog, FileFilter, Label, ListBox, Orientation,
    ScrolledWindow, SelectionMode, Stack,
};
use libadwaita::prelude::*;
use libadwaita::{
    AboutDialog, ActionRow, AlertDialog, Application, ApplicationWindow, Dialog as AdwDialog,
    ExpanderRow, HeaderBar, ResponseAppearance, StatusPage, Toast, ToastOverlay,
};

use std::cell::RefCell;
use std::rc::Rc;

// ─────────────────────────────────────────────────────────────────────────────
//  Async bridge: run a future on the shared Tokio runtime, deliver the result
//  back on the GTK main thread.
// ─────────────────────────────────────────────────────────────────────────────

fn spawn_async<F, T, CB>(future: F, callback: CB)
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
    CB: FnOnce(T) + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel::<T>();
    crate::runtime().spawn(async move {
        let _ = tx.send(future.await);
    });
    glib::MainContext::default().spawn_local(async move {
        if let Ok(val) = rx.await {
            callback(val);
        }
    });
}

// ─────────────────────────────────────────────────────────────────────────────
//  build_ui
// ─────────────────────────────────────────────────────────────────────────────

pub fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Proton Trainer")
        .default_width(480)
        .default_height(560)
        .build();

    let toast_overlay = ToastOverlay::new();

    // ── Header ───────────────────────────────────────────────────────────────
    let header = HeaderBar::new();

    let import_btn = Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Import trainer .exe")
        .build();
    header.pack_start(&import_btn);

    let stop_all_btn = Button::builder().label("Stop All").build();
    header.pack_start(&stop_all_btn);

    let about_btn = Button::builder()
        .icon_name("help-about-symbolic")
        .tooltip_text("About")
        .build();
    header.pack_end(&about_btn);

    let save_log_btn = Button::builder()
        .icon_name("document-save-symbolic")
        .tooltip_text("Save Debug Log")
        .build();
    header.pack_end(&save_log_btn);

    let troubleshoot_btn = Button::builder().label("Fix Stale Instance").build();
    header.pack_end(&troubleshoot_btn);

    // ── Stack: empty status page <-> trainer list ───────────────────────────
    let stack = Stack::new();

    let status_page = StatusPage::builder()
        .icon_name("input-gaming-symbolic")
        .title("No Trainers Yet")
        .description("Drop a trainer .exe here, or use the + button")
        .build();
    stack.add_named(&status_page, Some("empty"));

    let list_box = ListBox::new();
    list_box.set_selection_mode(SelectionMode::None);
    list_box.add_css_class("boxed-list");
    list_box.set_margin_top(12);
    list_box.set_margin_bottom(12);
    list_box.set_margin_start(12);
    list_box.set_margin_end(12);
    let list_scroll = ScrolledWindow::builder()
        .vexpand(true)
        .child(&list_box)
        .build();
    stack.add_named(&list_scroll, Some("list"));

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&header);
    content.append(&stack);
    toast_overlay.set_child(Some(&content));
    window.set_content(Some(&toast_overlay));

    // ─────────────────────────────────────────────────────────────────────────
    //  refresh_list — re-read the trainers dir and rebuild the list/rows
    // ─────────────────────────────────────────────────────────────────────────

    // Forward-declaration slot so the Remove button (defined inside the
    // closure being built) can call refresh_list once it exists.
    let self_slot: Rc<RefCell<Option<Rc<dyn Fn()>>>> = Rc::new(RefCell::new(None));

    let refresh_list: Rc<dyn Fn()> = {
        let list_box = list_box.clone();
        let stack = stack.clone();
        let window = window.clone();
        let toast_overlay = toast_overlay.clone();
        let self_slot = self_slot.clone();

        Rc::new(move || {
            while let Some(c) = list_box.first_child() {
                list_box.remove(&c);
            }

            let trainers = library::list_trainers().unwrap_or_default();
            if trainers.is_empty() {
                stack.set_visible_child_name("empty");
                return;
            }
            stack.set_visible_child_name("list");

            for trainer in trainers {
                let row = ActionRow::builder().title(&trainer.name).build();

                let launch_btn = Button::builder()
                    .label("Launch")
                    .valign(Align::Center)
                    .build();
                launch_btn.add_css_class("suggested-action");
                wire_launch_button(&launch_btn, trainer.clone(), window.clone(), toast_overlay.clone());
                row.add_suffix(&launch_btn);

                let remove_btn = Button::builder()
                    .icon_name("user-trash-symbolic")
                    .tooltip_text("Remove")
                    .valign(Align::Center)
                    .build();
                remove_btn.add_css_class("flat");
                {
                    let trainer = trainer.clone();
                    let toast_overlay = toast_overlay.clone();
                    let self_slot = self_slot.clone();
                    remove_btn.connect_clicked(move |_| {
                        if let Err(e) = library::remove_trainer(&trainer.path) {
                            toast_overlay.add_toast(Toast::new(&format!("Remove failed: {e}")));
                            return;
                        }
                        if let Some(reload) = self_slot.borrow().clone() {
                            reload();
                        }
                    });
                }
                row.add_suffix(&remove_btn);

                list_box.append(&row);
            }
        })
    };
    *self_slot.borrow_mut() = Some(refresh_list.clone());

    refresh_list();

    // ─────────────────────────────────────────────────────────────────────────
    //  Import — file picker
    // ─────────────────────────────────────────────────────────────────────────
    {
        let window = window.clone();
        let refresh_list = refresh_list.clone();
        let toast_overlay = toast_overlay.clone();
        import_btn.connect_clicked(move |_| {
            let filter = FileFilter::new();
            filter.add_pattern("*.exe");
            filter.set_name(Some("Trainer executable (.exe)"));
            let filters = gio::ListStore::new::<FileFilter>();
            filters.append(&filter);

            let dialog = FileDialog::builder()
                .title("Import Trainer")
                .filters(&filters)
                .build();

            let refresh_list = refresh_list.clone();
            let toast_overlay = toast_overlay.clone();
            dialog.open(Some(&window), gio::Cancellable::NONE, move |result| {
                let Ok(file) = result else { return };
                let Some(path) = file.path() else { return };
                let result = library::import_trainer(&path);
                applog::log(&format!("UI: import_trainer({}) -> {result:?}", path.display()));
                match result {
                    Ok(_) => {
                        refresh_list();
                        toast_overlay.add_toast(Toast::new("Trainer imported"));
                    }
                    Err(e) => toast_overlay.add_toast(Toast::new(&format!("Import failed: {e}"))),
                }
            });
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    //  Drop target — accept dropped .exe files anywhere in the window
    // ─────────────────────────────────────────────────────────────────────────
    {
        let drop_target = DropTarget::new(gdk4::FileList::static_type(), gdk4::DragAction::COPY);
        let refresh_list = refresh_list.clone();
        let toast_overlay = toast_overlay.clone();
        drop_target.connect_drop(move |_, value, _, _| {
            let Ok(file_list) = value.get::<gdk4::FileList>() else {
                return false;
            };
            let mut imported_any = false;
            for file in file_list.files() {
                let Some(path) = file.path() else { continue };
                let is_exe = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("exe"))
                    .unwrap_or(false);
                if !is_exe {
                    continue;
                }
                match library::import_trainer(&path) {
                    Ok(_) => imported_any = true,
                    Err(e) => toast_overlay.add_toast(Toast::new(&format!("Import failed: {e}"))),
                }
            }
            if imported_any {
                refresh_list();
                toast_overlay.add_toast(Toast::new("Trainer imported"));
            }
            imported_any
        });
        content.add_controller(drop_target);
    }

    // ─────────────────────────────────────────────────────────────────────────
    //  Stop All
    // ─────────────────────────────────────────────────────────────────────────
    {
        let toast_overlay = toast_overlay.clone();
        stop_all_btn.connect_clicked(move |_| {
            let toast_overlay = toast_overlay.clone();
            spawn_async(
                async {
                    tokio::task::spawn_blocking(|| {
                        if let Ok(dir) = library::trainers_dir() {
                            launcher::stop_all(&dir);
                        }
                    })
                    .await
                },
                move |_| {
                    toast_overlay.add_toast(Toast::new("Stopped all trainers"));
                },
            );
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    //  Fix Stale Instance
    // ─────────────────────────────────────────────────────────────────────────
    {
        let window = window.clone();
        let toast_overlay = toast_overlay.clone();
        troubleshoot_btn.connect_clicked(move |_| {
            show_troubleshoot(&window, &toast_overlay);
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    //  About
    // ─────────────────────────────────────────────────────────────────────────
    {
        let window = window.clone();
        about_btn.connect_clicked(move |_| {
            let dialog = AboutDialog::builder()
                .application_name("Proton Trainer")
                .version(env!("CARGO_PKG_VERSION"))
                .developers(vec!["Linnard Alex Brown Jr."])
                .comments(
                    "Launches Windows game trainers through Proton against a running \
                     Steam game's wine session.",
                )
                .build();
            dialog.present(Some(&window));
        });
    }

    // ─────────────────────────────────────────────────────────────────────────
    //  Save Debug Log — bundles the app log + privileged setup log (if
    //  readable) into one file the user picks, for handing to whoever's
    //  troubleshooting a failed launch.
    // ─────────────────────────────────────────────────────────────────────────
    {
        let window = window.clone();
        let toast_overlay = toast_overlay.clone();
        save_log_btn.connect_clicked(move |_| {
            let dialog = FileDialog::builder()
                .title("Save Debug Log")
                .initial_name(format!("proton-trainer-log-{}.txt", applog::filename_timestamp()))
                .build();

            let toast_overlay = toast_overlay.clone();
            dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
                let Ok(file) = result else { return };
                let Some(path) = file.path() else { return };
                match applog::export_to(&path) {
                    Ok(()) => toast_overlay.add_toast(Toast::new("Debug log saved")),
                    Err(e) => toast_overlay.add_toast(Toast::new(&format!("Save failed: {e}"))),
                }
            });
        });
    }

    window.present();
}

// ─────────────────────────────────────────────────────────────────────────────
//  Launch button wiring
// ─────────────────────────────────────────────────────────────────────────────

fn wire_launch_button(
    btn: &Button,
    trainer: Trainer,
    window: ApplicationWindow,
    toast_overlay: ToastOverlay,
) {
    btn.connect_clicked(move |_| {
        let trainer = trainer.clone();
        let window = window.clone();
        let toast_overlay = toast_overlay.clone();

        spawn_async(
            async { tokio::task::spawn_blocking(launcher::resolve_launch_target).await },
            move |result| {
                let target = match result {
                    Ok(Ok(t)) => t,
                    Ok(Err(e)) => {
                        toast_overlay.add_toast(Toast::new(&e.to_string()));
                        return;
                    }
                    Err(e) => {
                        toast_overlay.add_toast(Toast::new(&format!("Task error: {e}")));
                        return;
                    }
                };

                if !launcher::has_usable_dotnet(&target) {
                    show_dotnet_dialog(&window, target, trainer, toast_overlay);
                    return;
                }

                launch_trainer_now(target, trainer, toast_overlay);
            },
        );
    });
}

/// Launch a trainer against an already-resolved target, off the main
/// thread. Shared by the normal launch path and by the post-setup
/// auto-launch once the one-time .NET install succeeds.
fn launch_trainer_now(target: LaunchTarget, trainer: Trainer, toast_overlay: ToastOverlay) {
    let trainer_name = trainer.name.clone();
    let trainer_path = trainer.path.clone();
    let toast_overlay2 = toast_overlay.clone();
    let log = applog::log_path();
    let appid = target.appid;
    applog::log(&format!("UI: Launch clicked for trainer {trainer_name}"));

    spawn_async(
        async move {
            tokio::task::spawn_blocking(move || {
                launcher::launch_trainer(&target, &trainer_path, &log)
            })
            .await
        },
        move |launch_result| match launch_result {
            Ok(Ok(())) => toast_overlay2.add_toast(Toast::new(&format!(
                "Launched {trainer_name} (AppId {appid})"
            ))),
            Ok(Err(e)) => toast_overlay2.add_toast(Toast::new(&format!("Launch failed: {e}"))),
            Err(e) => toast_overlay2.add_toast(Toast::new(&format!("Task error: {e}"))),
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  .NET one-time setup dialog
// ─────────────────────────────────────────────────────────────────────────────

/// Exact manual commands, shown only inside the collapsed disclosure — the
/// default path is the "Set Up Automatically" button, which runs the same
/// commands itself via setup::run_system_setup / setup::install_dotnet48.
fn manual_commands(target: &LaunchTarget) -> String {
    format!(
        "dpkg --add-architecture i386 && apt update && apt install -y winetricks cabextract wine32:i386\n\
WINEPREFIX={} winetricks -q dotnet48 win10",
        target.prefix_dir().display()
    )
}

fn show_dotnet_dialog(
    window: &ApplicationWindow,
    target: LaunchTarget,
    trainer: Trainer,
    toast_overlay: ToastOverlay,
) {
    let body = "This game needs a one-time Windows compatibility component before trainers \
will run. This will ask for your password once, then take a minute or two.";

    let commands_label = Label::new(Some(&manual_commands(&target)));
    commands_label.set_wrap(true);
    commands_label.set_xalign(0.0);
    commands_label.set_selectable(true);
    commands_label.set_margin_top(6);
    commands_label.set_margin_bottom(6);
    commands_label.set_margin_start(12);
    commands_label.set_margin_end(12);

    let expander = ExpanderRow::builder()
        .title("Show manual commands instead")
        .expanded(false)
        .build();
    expander.add_row(&commands_label);

    let disclosure_list = ListBox::new();
    disclosure_list.set_selection_mode(SelectionMode::None);
    disclosure_list.add_css_class("boxed-list");
    disclosure_list.append(&expander);

    let dialog = AlertDialog::builder()
        .heading("One-Time Setup Needed")
        .body(body)
        .extra_child(&disclosure_list)
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("setup", "Set Up Automatically")]);
    dialog.set_response_appearance("setup", ResponseAppearance::Suggested);
    dialog.set_default_response(Some("setup"));
    dialog.set_close_response("cancel");

    // AlertDialog::connect_response requires Fn, not FnOnce, so the
    // non-Clone target/trainer are handed off through a RefCell rather than
    // moved directly — the dialog only ever fires one response in practice.
    let state: Rc<RefCell<Option<(LaunchTarget, Trainer)>>> =
        Rc::new(RefCell::new(Some((target, trainer))));

    dialog.connect_response(None, move |_dialog, response| {
        if response != "setup" {
            return;
        }
        let Some((target, trainer)) = state.borrow_mut().take() else {
            return;
        };
        run_automatic_setup(target, trainer, toast_overlay.clone());
    });

    dialog.present(Some(window));
}

/// Runs the two-phase setup (privileged system packages, then user-level
/// winetricks install) off the main thread, then auto-launches the trainer
/// on success. Errors from either phase surface via a toast with the real
/// error message; the setup script and install_dotnet48 both also log to
/// /var/log/proton-trainer.log.
fn run_automatic_setup(target: LaunchTarget, trainer: Trainer, toast_overlay: ToastOverlay) {
    toast_overlay.add_toast(Toast::new("Setting up .NET — this may take a minute or two…"));

    spawn_async(
        async move {
            tokio::task::spawn_blocking(move || -> anyhow::Result<LaunchTarget> {
                // Cloning from a prefix that already works is tried first, and
                // the Microsoft installer only as a fallback: on Wine's new
                // wow64 mode that installer fails outright and its rollback
                // strips .NET back out, leaving the prefix worse than it
                // started. Cloning needs no system packages either, so the
                // pkexec prompt is only reached when there's nothing to clone.
                if !launcher::repair_dotnet_from_sibling_prefix(&target)? {
                    if !setup::system_prereqs_present() {
                        setup::run_system_setup()?;
                    }
                    setup::install_dotnet48(&target.prefix_dir())?;
                }

                if !launcher::has_usable_dotnet(&target) {
                    anyhow::bail!(
                        "Couldn't get a working .NET runtime into this game's prefix. No other \
                         game's Proton prefix on this system had one to copy, and the installer \
                         didn't complete. See the debug log (Save Debug Log) for the details."
                    );
                }

                Ok(target)
            })
            .await
        },
        move |result| {
            let target = match result {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => {
                    toast_overlay.add_toast(Toast::new(&format!("Setup failed: {e}")));
                    return;
                }
                Err(e) => {
                    toast_overlay.add_toast(Toast::new(&format!("Task error: {e}")));
                    return;
                }
            };
            launch_trainer_now(target, trainer, toast_overlay);
        },
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  Stale-instance recovery dialog
// ─────────────────────────────────────────────────────────────────────────────

fn show_troubleshoot(window: &ApplicationWindow, toast_overlay: &ToastOverlay) {
    let window = window.clone();
    let toast_overlay = toast_overlay.clone();

    spawn_async(
        async { tokio::task::spawn_blocking(launcher::resolve_launch_target).await },
        move |result| {
            let target = match result {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => {
                    toast_overlay.add_toast(Toast::new(&e.to_string()));
                    return;
                }
                Err(e) => {
                    toast_overlay.add_toast(Toast::new(&format!("Task error: {e}")));
                    return;
                }
            };

            let dirs = launcher::trainer_log_dirs(&target);
            if dirs.is_empty() {
                toast_overlay.add_toast(Toast::new("No trainer logs found for the running game"));
                return;
            }

            let dialog = AdwDialog::builder()
                .title("Clear Stale Trainer Instance")
                .content_width(420)
                .content_height(320)
                .build();

            let outer = GtkBox::new(Orientation::Vertical, 8);
            outer.set_margin_top(12);
            outer.set_margin_bottom(12);
            outer.set_margin_start(12);
            outer.set_margin_end(12);

            let hint = Label::new(Some(
                "If a trainer refuses to start, claiming a previous instance is running, \
                 clear its log folder below, then relaunch.",
            ));
            hint.set_wrap(true);
            hint.set_xalign(0.0);
            outer.append(&hint);

            let list = ListBox::new();
            list.set_selection_mode(SelectionMode::None);
            list.add_css_class("boxed-list");
            outer.append(&list);

            for dir in dirs {
                let name = dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let row = ActionRow::builder().title(&name).build();

                let clear_btn = Button::builder()
                    .label("Clear")
                    .valign(Align::Center)
                    .build();
                clear_btn.add_css_class("destructive-action");

                let toast_overlay2 = toast_overlay.clone();
                let dir2 = dir.clone();
                clear_btn.connect_clicked(move |_| {
                    let result = std::fs::remove_file(dir2.join("info.ini"));
                    applog::log(&format!(
                        "UI: cleared stale instance {} -> {result:?}",
                        dir2.display()
                    ));
                    match result {
                        Ok(()) => toast_overlay2.add_toast(Toast::new("Cleared")),
                        Err(e) => {
                            toast_overlay2.add_toast(Toast::new(&format!("Failed: {e}")))
                        }
                    }
                });
                row.add_suffix(&clear_btn);
                list.append(&row);
            }

            let scroll = ScrolledWindow::builder().vexpand(true).child(&outer).build();
            dialog.set_child(Some(&scroll));
            dialog.present(Some(&window));
        },
    );
}
