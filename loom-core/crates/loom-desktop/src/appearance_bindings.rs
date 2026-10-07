//! Glue between the shared appearance choice and an application's own Slint
//! window.
//!
//! Every Slint window generates its own Rust types, so this glue cannot be an
//! ordinary generic function. [`appearance_bindings!`](crate::appearance_bindings)
//! expands inside an application module against that application's generated
//! window type. The choice, its resolution, and the settings file live in
//! [`crate::appearance`], tested once.

/// Expand the appearance bindings for one application window.
///
/// Invoke inside a module of the application crate:
/// `loom_desktop::appearance_bindings!(MyApp, set_status_left);`.
///
/// The crate root must export the shared `Theme` global from `loom-ui`
/// (`export { Theme } from "theme.slint";`). Generates:
///
/// * `on_change`: one listener that runs after every switch, so the menu's
///   check marks and any window that mirrors the theme (Present's presenter
///   window) stay in step;
/// * `install_store`, `start` and `start_with_store`: pick the saved choice
///   (or an explicit `--theme` flag, which is never written back) at startup;
/// * `apply` and `apply_id`: switch the window live without remembering;
/// * `choose`: switch, remember, and announce through the status line;
/// * `dispatch`: run a `view.appearance.*` command id from a menu, the
///   palette or the toolbar;
/// * `current` and `system_is_dark`: read the window's state.
///
/// Every application uses a different subset of these, and a macro cannot know
/// which, so the generated items (and only they) carry `allow(dead_code)`.
#[macro_export]
// The macro deliberately names the invoking crate's generated `Theme` global.
#[allow(clippy::crate_in_macro_def)]
macro_rules! appearance_bindings {
    ($app:ty, $status_setter:ident) => {
        ::std::thread_local! {
            static STORE: ::std::cell::RefCell<
                ::std::option::Option<$crate::appearance::AppearanceStore>,
            > = const { ::std::cell::RefCell::new(::std::option::Option::None) };
        }

        ::std::thread_local! {
            static LISTENER: ::std::cell::RefCell<
                ::std::option::Option<::std::rc::Rc<dyn Fn(&$app)>>,
            > = const { ::std::cell::RefCell::new(::std::option::Option::None) };
        }

        /// Run `listener` after every switch from now on.
        #[allow(dead_code)]
        pub(crate) fn on_change(listener: impl Fn(&$app) + 'static) {
            LISTENER.with(|slot| {
                *slot.borrow_mut() = ::std::option::Option::Some(::std::rc::Rc::new(listener))
            });
        }

        /// Remember choices in this store from now on.
        #[allow(dead_code)]
        pub(crate) fn install_store(store: $crate::appearance::AppearanceStore) {
            STORE.with(|slot| *slot.borrow_mut() = ::std::option::Option::Some(store));
        }

        /// The choice the window is showing.
        #[allow(dead_code)]
        pub(crate) fn current(app: &$app) -> $crate::appearance::Appearance {
            use ::slint::ComponentHandle as _;
            $crate::appearance::Appearance::from_id(
                app.global::<crate::Theme>().get_appearance().as_str(),
            )
            .unwrap_or($crate::appearance::Appearance::Light)
        }

        /// Whether the operating system reports a dark preference.
        #[allow(dead_code)]
        pub(crate) fn system_is_dark(app: &$app) -> bool {
            use ::slint::ComponentHandle as _;
            app.global::<crate::Theme>().get_system_scheme() == "dark"
        }

        /// Switch the window to a choice, without remembering it.
        #[allow(dead_code)]
        pub(crate) fn apply(app: &$app, choice: $crate::appearance::Appearance) {
            use ::slint::ComponentHandle as _;
            let theme = app.global::<crate::Theme>();
            theme.set_appearance(choice.id().into());
            theme.set_active_theme(choice.resolve(system_is_dark(app)).into());
            let listener = LISTENER.with(|slot| slot.borrow().clone());
            if let ::std::option::Option::Some(listener) = listener {
                listener(app);
            }
        }

        /// Switch to a named choice (`--theme`); an unknown name is light.
        #[allow(dead_code)]
        pub(crate) fn apply_id(app: &$app, name: &str) {
            apply(
                app,
                $crate::appearance::Appearance::from_id(name)
                    .unwrap_or($crate::appearance::Appearance::Light),
            );
        }

        /// Startup: use the explicit flag if given, otherwise the saved
        /// choice from `store`, and remember `store` for later changes.
        #[allow(dead_code)]
        pub(crate) fn start_with_store(
            app: &$app,
            store: $crate::appearance::AppearanceStore,
            flag: ::std::option::Option<&str>,
        ) {
            let choice = $crate::appearance::startup_appearance(&store, flag);
            install_store(store);
            apply(app, choice);
        }

        /// Startup with the per-user store of one application id.
        #[allow(dead_code)]
        pub(crate) fn start(app: &$app, application_id: &str, flag: ::std::option::Option<&str>) {
            start_with_store(
                app,
                $crate::appearance::AppearanceStore::for_application(application_id),
                flag,
            );
        }

        /// A user's choice: switch, remember, and announce it in the status
        /// line. A failed write is reported but the switch stays.
        #[allow(dead_code)]
        pub(crate) fn choose(app: &$app, choice: $crate::appearance::Appearance) {
            apply(app, choice);
            let saved = STORE.with(|slot| slot.borrow().as_ref().map(|store| store.save(choice)));
            let mut message = choice.announcement(system_is_dark(app));
            if let ::std::option::Option::Some(::std::result::Result::Err(error)) = saved {
                message.push_str(&::std::format!(" (could not be remembered: {error})"));
            }
            app.$status_setter(::slint::SharedString::from(message));
        }

        /// Run a `view.appearance.*` command; false for any other id.
        #[allow(dead_code)]
        pub(crate) fn dispatch(app: &$app, id: &str) -> bool {
            match $crate::appearance::Appearance::from_command_id(id) {
                ::std::option::Option::Some(choice) => {
                    choose(app, choice);
                    true
                }
                ::std::option::Option::None => false,
            }
        }
    };
}
