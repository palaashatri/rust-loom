use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::mpsc::RecvTimeoutError;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
#[cfg(test)]
use std::time::Duration;
use std::{cell::Cell, cell::RefCell};

use super::*;
use crate::workbook_io::LoadedWorkbook;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpenOperation {
    operation_id: u64,
    document_generation: u64,
    pub(crate) target_revision: u64,
    native_write_epoch: u64,
    authorized_dirty_revision: Option<u64>,
}

#[derive(Default)]
pub(crate) struct OpenOperationCoordinator {
    next_operation_id: u64,
    document_generation: u64,
    active: Option<OpenOperation>,
}

impl OpenOperationCoordinator {
    pub(crate) fn begin(&mut self, target_revision: u64, native_write_epoch: u64) -> OpenOperation {
        self.next_operation_id = self
            .next_operation_id
            .checked_add(1)
            .expect("Sheets Open operation id exhausted");
        let operation = OpenOperation {
            operation_id: self.next_operation_id,
            document_generation: self.document_generation,
            target_revision,
            native_write_epoch,
            authorized_dirty_revision: None,
        };
        self.active = Some(operation);
        operation
    }

    pub(crate) fn is_current(&self, operation: OpenOperation) -> bool {
        self.active.is_some_and(|active| {
            active.operation_id == operation.operation_id
                && active.document_generation == operation.document_generation
                && operation.document_generation == self.document_generation
        })
    }

    pub(crate) fn document_replaced(&mut self) {
        self.document_generation = self
            .document_generation
            .checked_add(1)
            .expect("Sheets document generation exhausted");
        self.active = None;
    }

    pub(crate) fn cancel(&mut self, operation: OpenOperation) {
        if self.is_current(operation) {
            self.active = None;
        }
    }
}

pub(crate) struct OpenFileCompletion {
    pub(crate) operation: OpenOperation,
    pub(crate) path: PathBuf,
    pub(crate) result: Result<LoadedWorkbook, String>,
    pub(crate) startup_options: Option<StartupOpenOptions>,
}

#[derive(Clone, Copy)]
pub(crate) struct StartupOpenOptions {
    objects: bool,
    chart: bool,
    template_chooser: bool,
}

impl StartupOpenOptions {
    pub(crate) fn new(objects: bool, chart: bool, template_chooser: bool) -> Self {
        Self {
            objects,
            chart,
            template_chooser,
        }
    }
}

type WorkbookLoader = Box<dyn FnOnce(&Path) -> Result<LoadedWorkbook, String> + Send>;

struct OpenLoadJob {
    operation: OpenOperation,
    path: PathBuf,
    startup_options: Option<StartupOpenOptions>,
    load: WorkbookLoader,
}

#[derive(Default)]
struct OpenLoadState {
    pending: Option<OpenLoadJob>,
    shutdown: bool,
}

#[derive(Default)]
struct OpenLoadQueue {
    state: Mutex<OpenLoadState>,
    ready: Condvar,
}

pub(crate) struct OpenCompletionQueue {
    sender: Sender<OpenFileCompletion>,
    receiver: Receiver<OpenFileCompletion>,
    work: Arc<OpenLoadQueue>,
    worker_started: Cell<bool>,
    worker: RefCell<Option<JoinHandle<()>>>,
}

impl OpenCompletionQueue {
    pub(crate) fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            work: Arc::new(OpenLoadQueue::default()),
            worker_started: Cell::new(false),
            worker: RefCell::new(None),
        }
    }

    pub(crate) fn start_load(&self, operation: OpenOperation, path: PathBuf) -> Result<(), String> {
        self.schedule(
            operation,
            path,
            None,
            crate::workbook_io::load_workbook_with_report,
        )
    }

    pub(crate) fn start_startup_load(
        &self,
        operation: OpenOperation,
        path: PathBuf,
        options: StartupOpenOptions,
    ) -> Result<(), String> {
        self.schedule(operation, path, Some(options), move |path| {
            let mut loaded = crate::workbook_io::load_workbook_with_report(path)?;
            prepare_startup_candidate(&mut loaded.workbook, options);
            Ok(loaded)
        })
    }

    #[cfg(test)]
    fn start_load_with<F>(
        &self,
        operation: OpenOperation,
        path: PathBuf,
        load: F,
    ) -> Result<(), String>
    where
        F: FnOnce(&Path) -> Result<LoadedWorkbook, String> + Send + 'static,
    {
        self.schedule(operation, path, None, load)
    }

    fn schedule<F>(
        &self,
        operation: OpenOperation,
        path: PathBuf,
        startup_options: Option<StartupOpenOptions>,
        load: F,
    ) -> Result<(), String>
    where
        F: FnOnce(&Path) -> Result<LoadedWorkbook, String> + Send + 'static,
    {
        self.start_worker()?;
        let mut state = self
            .work
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.shutdown {
            return Err("workbook open queue has stopped".into());
        }
        // There is one running loader and one newest pending open. A newer
        // request makes any waiting request stale in the coordinator.
        state.pending = Some(OpenLoadJob {
            operation,
            path,
            startup_options,
            load: Box::new(load),
        });
        drop(state);
        self.work.ready.notify_one();
        Ok(())
    }

    fn start_worker(&self) -> Result<(), String> {
        if self.worker_started.get() {
            return Ok(());
        }
        let work = Arc::clone(&self.work);
        let sender = self.sender.clone();
        let worker = thread::Builder::new()
            .name("loom-sheets-open".into())
            .spawn(move || run_open_loader(work, sender))
            .map_err(|error| format!("start workbook open worker: {error}"))?;
        *self.worker.borrow_mut() = Some(worker);
        self.worker_started.set(true);
        Ok(())
    }

    pub(crate) fn try_receive(&self) -> Option<OpenFileCompletion> {
        self.receiver.try_recv().ok()
    }

    #[cfg(test)]
    fn receive_timeout(&self, timeout: Duration) -> Result<OpenFileCompletion, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}

impl Drop for OpenCompletionQueue {
    fn drop(&mut self) {
        let mut state = self
            .work
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.shutdown = true;
        state.pending = None;
        drop(state);
        self.work.ready.notify_all();
        self.worker.get_mut().take();
    }
}

fn run_open_loader(work: Arc<OpenLoadQueue>, sender: Sender<OpenFileCompletion>) {
    loop {
        let job = {
            let mut state = work
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                if state.shutdown {
                    return;
                }
                if let Some(job) = state.pending.take() {
                    break job;
                }
                state = work
                    .ready
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        let result = (job.load)(&job.path);
        if sender
            .send(OpenFileCompletion {
                operation: job.operation,
                path: job.path,
                result,
                startup_options: job.startup_options,
            })
            .is_err()
        {
            return;
        }
    }
}

#[derive(Default)]
pub(crate) struct OpenOperations {
    coordinator: OpenOperationCoordinator,
    completions: OpenCompletionQueue,
    pending_candidate: Option<OpenFileCompletion>,
    native_write_epoch: u64,
}

impl Default for OpenCompletionQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenOperations {
    pub(crate) fn begin_operation(&mut self, target_revision: u64) -> OpenOperation {
        self.pending_candidate = None;
        self.coordinator
            .begin(target_revision, self.native_write_epoch)
    }

    pub(crate) fn note_successful_native_write(&mut self) {
        self.native_write_epoch = self
            .native_write_epoch
            .checked_add(1)
            .expect("Sheets native write epoch exhausted");
    }

    fn candidate_needs_reload(&self, operation: OpenOperation) -> bool {
        self.coordinator.is_current(operation)
            && operation.native_write_epoch != self.native_write_epoch
    }

    pub(crate) fn start_picker_load(
        &mut self,
        path: PathBuf,
        target_revision: u64,
    ) -> Result<OpenOperation, String> {
        let operation = self.begin_operation(target_revision);
        if let Err(error) = self.completions.start_load(operation, path) {
            self.coordinator.cancel(operation);
            return Err(error);
        }
        Ok(operation)
    }

    #[cfg(test)]
    pub(crate) fn start_picker_load_with<F>(
        &mut self,
        path: PathBuf,
        target_revision: u64,
        load: F,
    ) -> Result<OpenOperation, String>
    where
        F: FnOnce(&Path) -> Result<LoadedWorkbook, String> + Send + 'static,
    {
        let operation = self.begin_operation(target_revision);
        if let Err(error) = self.completions.start_load_with(operation, path, load) {
            self.coordinator.cancel(operation);
            return Err(error);
        }
        Ok(operation)
    }

    pub(crate) fn start_startup_load(
        &mut self,
        path: PathBuf,
        target_revision: u64,
        options: StartupOpenOptions,
    ) -> Result<OpenOperation, String> {
        let operation = self.begin_operation(target_revision);
        if let Err(error) = self
            .completions
            .start_startup_load(operation, path, options)
        {
            self.coordinator.cancel(operation);
            return Err(error);
        }
        Ok(operation)
    }

    pub(crate) fn document_generation(&self) -> u64 {
        self.coordinator.document_generation
    }

    pub(crate) fn is_current(&self, operation: OpenOperation) -> bool {
        self.coordinator.is_current(operation)
    }

    pub(super) fn allows_dirty_replacement(
        &self,
        operation: OpenOperation,
        current_revision: u64,
    ) -> bool {
        self.coordinator.is_current(operation)
            && self
                .coordinator
                .active
                .is_some_and(|active| active.authorized_dirty_revision == Some(current_revision))
    }

    pub(crate) fn authorize_dirty_replacement(&mut self, operation: OpenOperation, revision: u64) {
        if !self.coordinator.is_current(operation) {
            return;
        }
        if let Some(active) = self.coordinator.active.as_mut() {
            active.authorized_dirty_revision = Some(revision);
        }
    }

    fn authorized_dirty_revision(&self, operation: OpenOperation) -> Option<u64> {
        self.coordinator
            .is_current(operation)
            .then(|| {
                self.coordinator
                    .active
                    .and_then(|active| active.authorized_dirty_revision)
            })
            .flatten()
    }

    pub(crate) fn acknowledge_revision(
        &mut self,
        operation: OpenOperation,
        revision: u64,
        authorized_dirty_revision: Option<u64>,
    ) -> Option<OpenOperation> {
        if !self.coordinator.is_current(operation) {
            return None;
        }
        let active = self.coordinator.active.as_mut()?;
        active.target_revision = revision;
        active.authorized_dirty_revision = authorized_dirty_revision;
        Some(*active)
    }

    pub(crate) fn cancel(&mut self, operation: OpenOperation) {
        self.coordinator.cancel(operation);
        if self
            .pending_candidate
            .as_ref()
            .is_some_and(|candidate| candidate.operation == operation)
        {
            self.pending_candidate = None;
        }
    }

    pub(crate) fn document_replaced(&mut self) {
        self.coordinator.document_replaced();
        self.pending_candidate = None;
    }

    fn drain(&self) -> Vec<OpenFileCompletion> {
        std::iter::from_fn(|| self.completions.try_receive()).collect()
    }

    fn hold_candidate(&mut self, completion: OpenFileCompletion) {
        self.pending_candidate = Some(completion);
    }

    fn take_candidate(&mut self) -> Option<OpenFileCompletion> {
        self.pending_candidate.take()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingReplacement {
    NewWorkbook,
    OpenWorkbook,
    OpenCandidate,
}

pub(super) fn open_request(state: &GuiState) -> OpenFileRequest {
    OpenFileRequest {
        title: "Open or Import Loom Sheets Workbook".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: None,
        filters: vec![
            state.workbook_filter.clone(),
            state.import_filter.clone(),
            state.xlsx_filter.clone(),
        ],
    }
}

pub(super) fn is_native_workbook(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("loomtable"))
}

pub(super) fn replace_opened_workbook(
    app: &SheetsApp,
    state: &GuiState,
    path: PathBuf,
    sheets: Vec<Sheet>,
    active: usize,
) {
    state.open_operations.borrow_mut().document_replaced();
    let active = active.min(sheets.len().saturating_sub(1));
    *state.current.borrow_mut() = sheets[active].clone();
    *state.sheets.borrow_mut() = sheets;
    *state.active_sheet_index.borrow_mut() = active;
    *state.save_path.borrow_mut() = is_native_workbook(&path).then_some(path);
    state.undo_stack.borrow_mut().clear();
    state.redo_stack.borrow_mut().clear();
    *state.sheet_histories.borrow_mut() =
        vec![(Vec::new(), Vec::new()); state.sheets.borrow().len()];
    apply_sheet(app, state);
    sync_sheet_tabs(app, state);
    state.mark_saved();
    sync_window_title(app, state);
}

pub(super) fn begin_new_workbook(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) {
    state.open_operations.borrow_mut().document_replaced();
    let sheet = blank_sheet();
    *state.current.borrow_mut() = sheet.clone();
    *state.sheets.borrow_mut() = vec![sheet];
    *state.active_sheet_index.borrow_mut() = 0;
    *state.save_path.borrow_mut() = None;
    state.undo_stack.borrow_mut().clear();
    state.redo_stack.borrow_mut().clear();
    *state.sheet_histories.borrow_mut() = vec![(Vec::new(), Vec::new())];
    apply_sheet(app, state);
    sync_sheet_tabs(app, state);
    state.mark_saved();
    sync_window_title(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left("Created new unsaved workbook".into());
}

fn prepare_startup_candidate(workbook: &mut WorkbookFile, options: StartupOpenOptions) {
    if workbook.sheets.is_empty() {
        workbook.sheets.push(blank_sheet());
    }
    workbook.active = workbook.active.min(workbook.sheets.len() - 1);
    let sheet = &mut workbook.sheets[workbook.active];
    if options.objects {
        object_actions::seed_demo_objects(sheet);
    }
    if options.chart {
        let chart = if sheet.name == "Example Budget" {
            plan_chart_in_range(sheet, 0, 1, 1, 3).ok()
        } else {
            plan_chart(sheet, 0, 1).ok()
        };
        if let Some(chart) = chart {
            sheet.chart = Some(chart);
        }
    }
}

pub(super) fn apply_startup_projection(
    app: &SheetsApp,
    state: &GuiState,
    options: StartupOpenOptions,
) {
    if options.objects {
        app.set_selected_object(0);
    }
    if options.chart && state.current.borrow().chart.is_some() {
        app.set_chart_visible(true);
        sync_chart_to_app(app, &state.current.borrow());
    }
    if options.template_chooser {
        app.set_template_chooser_open(true);
        app.invoke_focus_template_chooser();
    }
}

pub(super) fn open_workbook_from_picker(
    app: &SheetsApp,
    state: &GuiState,
    _menu_service: &Arc<loom_desktop::NativeMenuBar>,
    authorized_dirty_revision: Option<u64>,
) {
    match state.dialogs.open_file(&open_request(state)) {
        Ok(Some(path)) => {
            let open_text = format!("Opening {}…", path.display());
            let result = state
                .open_operations
                .borrow_mut()
                .start_picker_load(path, state.worker_revision.get());
            match result {
                Ok(operation) => {
                    if let Some(revision) = authorized_dirty_revision {
                        state
                            .open_operations
                            .borrow_mut()
                            .authorize_dirty_replacement(operation, revision);
                    }
                    app.set_status_left(SharedString::from(open_text));
                }
                Err(error) => {
                    app.set_status_left(SharedString::from(format!("Open failed: {error}")))
                }
            }
        }
        Ok(None) => app.set_status_left("Open cancelled".into()),
        Err(error) => {
            app.set_status_left(SharedString::from(format!("Open dialog failed: {error}")))
        }
    }
}

pub(super) fn start_startup_open(
    app: &SheetsApp,
    state: &GuiState,
    path: PathBuf,
    options: StartupOpenOptions,
) {
    let status = format!("Opening {}…", path.display());
    let result = state.open_operations.borrow_mut().start_startup_load(
        path,
        state.worker_revision.get(),
        options,
    );
    match result {
        Ok(_) => app.set_status_left(SharedString::from(status)),
        Err(error) => app.set_status_left(SharedString::from(format!("Open failed: {error}"))),
    }
}

fn handle_completion(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
    completion: OpenFileCompletion,
) {
    if !state
        .open_operations
        .borrow()
        .is_current(completion.operation)
    {
        return;
    }
    let OpenFileCompletion {
        operation,
        path,
        result,
        startup_options,
    } = completion;
    if reload_candidate_after_save_if_needed(app, state, path.clone(), operation, startup_options) {
        return;
    }
    let loaded = match result {
        Ok(loaded) => loaded,
        Err(error) => {
            state.open_operations.borrow_mut().cancel(operation);
            app.set_status_left(SharedString::from(format!("Open failed: {error}")));
            return;
        }
    };
    let dirty_replacement_allowed = state
        .open_operations
        .borrow()
        .allows_dirty_replacement(operation, state.worker_revision.get());
    if (state.is_dirty() && !dirty_replacement_allowed) || has_formula_draft(app) {
        state
            .open_operations
            .borrow_mut()
            .hold_candidate(OpenFileCompletion {
                operation,
                path,
                result: Ok(loaded),
                startup_options,
            });
        request_workbook_replacement(app, state, PendingReplacement::OpenCandidate);
        return;
    }
    let accepted_without_warning = loaded.warnings.is_empty();
    xlsx_import::handle_loaded_workbook(
        app,
        state,
        menu_service,
        path,
        loaded,
        operation,
        startup_options,
    );
    if accepted_without_warning {
        if let Some(options) = startup_options {
            apply_startup_projection(app, state, options);
        }
    }
}

pub(super) fn process_completions(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) -> usize {
    if state.save_operations.borrow().is_active() {
        return 0;
    }
    let completions = state.open_operations.borrow().drain();
    let count = completions.len();
    for completion in completions {
        handle_completion(app, state, menu_service, completion);
    }
    count
}

pub(super) fn reload_candidate_after_save_if_needed(
    app: &SheetsApp,
    state: &GuiState,
    path: PathBuf,
    operation: OpenOperation,
    startup_options: Option<StartupOpenOptions>,
) -> bool {
    let (candidate_needs_reload, authorized_dirty_revision) = {
        let operations = state.open_operations.borrow();
        (
            operations.candidate_needs_reload(operation),
            operations.authorized_dirty_revision(operation),
        )
    };
    if !candidate_needs_reload {
        return false;
    }

    let revision = state.worker_revision.get();
    let result = {
        let mut operations = state.open_operations.borrow_mut();
        match startup_options {
            Some(options) => operations.start_startup_load(path, revision, options),
            None => operations.start_picker_load(path, revision),
        }
    };
    match result {
        Ok(reloaded_operation) => {
            if let Some(revision) = authorized_dirty_revision {
                state
                    .open_operations
                    .borrow_mut()
                    .authorize_dirty_replacement(reloaded_operation, revision);
            }
            app.set_status_left(
                "The workbook was saved while it was opening; reloading the saved file".into(),
            )
        }
        Err(error) => app.set_status_left(SharedString::from(format!(
            "Open failed while reloading the saved file: {error}"
        ))),
    }
    true
}

pub(super) fn start_completion_timer(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) -> slint::Timer {
    let timer = slint::Timer::default();
    let app_ref = app.as_weak();
    let state = Rc::clone(state);
    let menu_service = Arc::clone(menu_service);
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            if let Some(app) = app_ref.upgrade() {
                process_completions(&app, &state, &menu_service);
            }
        },
    );
    timer
}

fn request_workbook_replacement(
    app: &SheetsApp,
    state: &GuiState,
    replacement: PendingReplacement,
) -> bool {
    if state.pending_replacement.get().is_some() {
        return true;
    }
    if state.save_operations.borrow().is_active() {
        app.set_status_left(
            "Save in progress — wait for it to finish before replacing this workbook".into(),
        );
        return true;
    }
    if !state.is_dirty() && !has_formula_draft(app) {
        return false;
    }
    state.pending_replacement.set(Some(replacement));
    state.advance_pending_replacement_token();
    app.set_save_changes_document(SharedString::from(workbook_display_name(state)));
    app.set_save_changes_open(true);
    app.set_status_left("Unsaved changes — choose Save, Discard, or Cancel".into());
    true
}

pub(super) fn has_formula_draft(app: &SheetsApp) -> bool {
    app.get_formula_edit_buffer() != app.get_selection_formula()
}

pub(super) fn save_changes_and_resume(
    app: &SheetsApp,
    state: &GuiState,
    _menu_service: &Arc<loom_desktop::NativeMenuBar>,
) -> Result<bool, String> {
    super::save_current_sheet(app, state, false)
}

pub(super) fn discard_changes_and_resume(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) {
    if state.save_operations.borrow().is_active() {
        app.set_status_left(
            "Save in progress — wait for it to finish before discarding this workbook".into(),
        );
        return;
    }
    app.invoke_reset_formula_edit_buffer();
    app.set_save_changes_open(false);
    let authorized_dirty_revision = state.worker_revision.get();
    continue_pending_replacement(app, state, menu_service, Some(authorized_dirty_revision));
}

fn resume_open_candidate(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
    authorized_dirty_revision: Option<u64>,
) {
    let candidate = state.open_operations.borrow_mut().take_candidate();
    if let Some(mut completion) = candidate {
        let ticket = state.open_operations.borrow_mut().acknowledge_revision(
            completion.operation,
            state.worker_revision.get(),
            authorized_dirty_revision,
        );
        let Some(ticket) = ticket else { return };
        completion.operation = ticket;
        handle_completion(app, state, menu_service, completion);
        return;
    }

    let ticket = state
        .pending_xlsx_import
        .borrow()
        .as_ref()
        .and_then(|pending| pending.operation);
    if let Some(ticket) = ticket {
        let Some(ticket) = state.open_operations.borrow_mut().acknowledge_revision(
            ticket,
            state.worker_revision.get(),
            authorized_dirty_revision,
        ) else {
            return;
        };
        if let Some(pending) = state.pending_xlsx_import.borrow_mut().as_mut() {
            pending.operation = Some(ticket);
        }
    }
    xlsx_import::continue_pending_xlsx_import(app, state, menu_service);
}

fn continue_pending_replacement(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
    authorized_dirty_revision: Option<u64>,
) {
    let pending = state.pending_replacement.take();
    if pending.is_some() {
        state.advance_pending_replacement_token();
    }
    match pending {
        Some(PendingReplacement::NewWorkbook) => begin_new_workbook(app, state, menu_service),
        Some(PendingReplacement::OpenWorkbook) => {
            open_workbook_from_picker(app, state, menu_service, authorized_dirty_revision)
        }
        Some(PendingReplacement::OpenCandidate) => {
            resume_open_candidate(app, state, menu_service, authorized_dirty_revision)
        }
        None => {}
    }
}

pub(super) fn cancel_pending_replacement(app: &SheetsApp, state: &GuiState) {
    if state.pending_replacement.get() == Some(PendingReplacement::OpenCandidate) {
        let candidate = state.open_operations.borrow_mut().take_candidate();
        if let Some(candidate) = candidate {
            state
                .open_operations
                .borrow_mut()
                .cancel(candidate.operation);
        }
        if let Some(pending) = state.pending_xlsx_import.borrow_mut().take() {
            if let Some(operation) = pending.operation {
                state.open_operations.borrow_mut().cancel(operation);
            }
            app.set_xlsx_import_warning_open(false);
            app.set_xlsx_import_warning_message(SharedString::new());
        }
    }
    if state.pending_replacement.take().is_some() {
        state.advance_pending_replacement_token();
    }
}

pub(super) fn cancel_save_changes_dialog(app: &SheetsApp, state: &GuiState) {
    let save_in_progress = state.save_operations.borrow().is_active();
    cancel_pending_replacement(app, state);
    app.set_save_changes_open(false);
    app.set_status_left(if save_in_progress {
        "Replacement cancelled; the Save already in progress will continue".into()
    } else {
        "Replacement cancelled; your workbook is unchanged".into()
    });
}

pub(super) fn continue_pending_replacement_after_dialog(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) {
    continue_pending_replacement(app, state, menu_service, None);
}

pub(super) fn request_replacement_after_dialog(
    app: &SheetsApp,
    state: &GuiState,
    replacement: PendingReplacement,
) -> bool {
    request_workbook_replacement(app, state, replacement)
}

pub(super) fn start_file_timer_after_show(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) -> slint::Timer {
    start_completion_timer(app, state, menu_service)
}

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) fn has_held_candidate(operations: &super::OpenOperations) -> bool {
        operations.pending_candidate.is_some()
    }

    pub(crate) fn current_operation(
        operations: &super::OpenOperations,
    ) -> Option<super::OpenOperation> {
        operations.coordinator.active
    }
}

#[cfg(test)]
#[path = "open_operations_tests.rs"]
mod tests;
