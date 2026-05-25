#include "master.hpp"

namespace factor {

// Compile code in boot image so that we can execute the startup quotation
// Allocates memory
void factor_vm::prepare_boot_image() {
  std::cout << "*** Stage 2 early init... " << std::flush;

  // Compile all words.
  data_root<array> words(instances(WORD_TYPE), this);

  cell n_words = array_capacity(words.untagged());
  for (cell i = 0; i < n_words; i++) {
    data_root<word> word(array_nth(words.untagged(), i), this);

    FACTOR_ASSERT(!word->entry_point);
    jit_compile_word(word.value(), word->def, false);
  }
  update_code_heap_words(true);

  // Initialize all quotations
  data_root<array> quotations(instances(QUOTATION_TYPE), this);

  cell n_quots = array_capacity(quotations.untagged());
  for (cell i = 0; i < n_quots; i++) {
    data_root<quotation> quot(array_nth(quotations.untagged(), i), this);

    if (!quot->entry_point)
      quot->entry_point = lazy_jit_compile_entry_point();
  }

  special_objects[OBJ_STAGE2] = special_objects[OBJ_CANONICAL_TRUE];

  std::cout << "done" << std::endl;
}

void factor_vm::init_factor(vm_parameters* p) {
  // Kilobytes
  p->datastack_size = align_page(p->datastack_size << 10);
  p->retainstack_size = align_page(p->retainstack_size << 10);
  p->callstack_size = align_page(p->callstack_size << 10);
  p->callback_size = align_page(p->callback_size << 10);

  // Megabytes
  p->young_size <<= 20;
  p->aging_size <<= 20;
  p->tenured_size <<= 20;
  p->code_size <<= 20;

  // Disable GC during init as a sanity check
  gc_off = true;

  // OS-specific initialization
  early_init();

  p->executable_path = vm_executable_path();

  if (p->image_path == NULL) {
    if (embedded_image_p()) {
      p->embedded_image = true;
      p->image_path = safe_strdup(p->executable_path);
    } else
      p->image_path = default_image_path();
  }

  srand((unsigned int)nano_count());
  init_ffi();

  datastack_size = p->datastack_size;
  retainstack_size = p->retainstack_size;
  callstack_size = p->callstack_size;

  ctx = NULL;
  spare_ctx = new_context();

  callbacks = new callback_heap(p->callback_size, this);
  load_image(p);
  max_pic_size = (int)p->max_pic_size;
  special_objects[OBJ_CELL_SIZE] = tag_fixnum(sizeof(cell));
  special_objects[OBJ_ARGS] = false_object;
  special_objects[OBJ_EMBEDDED] = false_object;

#ifdef WINDOWS
#define NO_ASSOCIATED_STREAM -2
#define VALID_HANDLE(handle,mode) (_fileno (handle)!= NO_ASSOCIATED_STREAM ? handle : fopen ("nul",(mode)))
#else
#define VALID_HANDLE(handle,mode) (handle)
#endif

  cell aliens[][2] = {
    {OBJ_STDIN,           (cell)(VALID_HANDLE (stdin ,"r"))},
    {OBJ_STDOUT,          (cell)(VALID_HANDLE (stdout,"w"))},
    {OBJ_STDERR,          (cell)(VALID_HANDLE (stderr,"w"))},
    {OBJ_CPU,             (cell)FACTOR_CPU_STRING},
    {OBJ_EXECUTABLE,      (cell)safe_strdup(p->executable_path)},
    {OBJ_IMAGE,           (cell)safe_strdup(p->image_path)},
    {OBJ_OS,              (cell)FACTOR_OS_STRING},
    {OBJ_VM_COMPILE_TIME, (cell)FACTOR_COMPILE_TIME},
    {OBJ_VM_COMPILER,     (cell)FACTOR_COMPILER_VERSION},
    {OBJ_VM_GIT_LABEL,    (cell)FACTOR_STRINGIZE(FACTOR_GIT_LABEL)},
    {OBJ_VM_VERSION,      (cell)FACTOR_STRINGIZE(FACTOR_VERSION)},
#if defined(WINDOWS)
    {WIN_EXCEPTION_HANDLER, (cell)&factor::exception_handler}
#endif
  };
  int n_items = sizeof(aliens) / sizeof(cell[2]);
  for (int n = 0; n < n_items; n++) {
    cell idx = aliens[n][0];
    special_objects[idx] = allot_alien(false_object, aliens[n][1]);
  }

  // We can GC now
  gc_off = false;

  if (!to_boolean(special_objects[OBJ_STAGE2]))
    prepare_boot_image();

  if (p->signals)
    init_signals();

  if (p->console)
    open_console();

}

// Allocates memory
void factor_vm::pass_args_to_factor(int argc, vm_char** argv) {
  growable_array args(this);

  for (fixnum i = 0; i < argc; i++)
    args.add(allot_alien(false_object, (cell)argv[i]));

  args.trim();
  special_objects[OBJ_ARGS] = args.elements.value();
}

void factor_vm::stop_factor() {
  c_to_factor_toplevel(special_objects[OBJ_SHUTDOWN_QUOT]);
}

char* factor_vm::factor_eval_string(char* string) {
  // Register the SEH unwind table for the JIT'd code segment
  // before dispatching into the eval-callback.  Same mechanism
  // c_to_factor_toplevel uses for STARTUP/SHUTDOWN quots, but
  // those bypass this entry point.  Without the registration,
  // a guard-page fault (e.g. data-stack underflow) inside the
  // callback has no SEH handler to find - Windows treats it as
  // an unhandled exception and terminates the process.  With it,
  // the fault unwinds to Factor's exception_handler which
  // converts it into ERROR_DATASTACK_UNDERFLOW etc. and the
  // listener's recover catches cleanly.
#if defined(WINDOWS) && defined(FACTOR_64)
  install_seh_table();
#endif
  void* func = alien_offset(special_objects[OBJ_EVAL_CALLBACK]);
  CODE_TO_FUNCTION_POINTER(func);
  char* result = ((char * (*)(char*)) func)(string);
#if defined(WINDOWS) && defined(FACTOR_64)
  uninstall_seh_table();
#endif
  return result;
}

void factor_vm::factor_eval_free(char* result) { free(result); }

void factor_vm::factor_yield() {
  void* func = alien_offset(special_objects[OBJ_YIELD_CALLBACK]);
  CODE_TO_FUNCTION_POINTER(func);
  ((void(*)()) func)();
}

void factor_vm::factor_sleep(long us) {
  void* func = alien_offset(special_objects[OBJ_SLEEP_CALLBACK]);
  CODE_TO_FUNCTION_POINTER(func);
  ((void(*)(long)) func)(us);
}

void factor_vm::start_standalone_factor(int argc, vm_char** argv) {
  vm_parameters p;
  p.init_from_args(argc, argv);
  init_factor(&p);
  pass_args_to_factor(argc, argv);

  if (p.fep)
    factorbug();

  c_to_factor_toplevel(special_objects[OBJ_STARTUP_QUOT]);
}

factor_vm* new_factor_vm() {
  THREADHANDLE thread = thread_id();
  factor_vm* newvm = new factor_vm(thread);
  register_vm_with_thread(newvm);
  thread_vms[thread] = newvm;

  return newvm;
}

VM_C_API void start_standalone_factor(int argc, vm_char** argv) {
  factor_vm* newvm = new_factor_vm();
  newvm->start_standalone_factor(argc, argv);
  delete newvm;
}

// ── NewFactor embedding API ─────────────────────────────────────────────
//
// Trivial VM_C_API pass-throughs exposing factor_vm's existing embedding
// methods as exported DLL symbols.  See E:\NewFactor\docs\embedding-api-findings.md
// for design rationale.  These add no semantics; they just dllexport
// methods Slava already wrote so a host process can drive Factor without
// going through start_standalone_factor's command-line-startup path.
//
// The `nf_` prefix avoids any chance of collision with future Factor C-API
// additions and signals "added by NewFactor for embedding".

VM_C_API factor_vm* nf_new_vm(void) {
  // init_mvm() allocates the TLS slot used by current_vm_p().  This is
  // normally called by main-windows.cpp's wmain entry point, which
  // factor.com / factor.exe use but the DLL never sees.  Without it,
  // current_vm_tls_key stays at 0 (default), register_vm_with_thread
  // writes to TLS slot 0 (invalid), and every later JIT call that uses
  // `current-vm` reads garbage from TLS → ERROR_MEMORY on first
  // dereference.  Calling it twice is harmless on Windows (TlsAlloc
  // would just allocate another slot, but we guard so it only runs
  // once).
  static int initialised = 0;
  if (!initialised) { init_mvm(); initialised = 1; }
  return new_factor_vm();
}

// Heap-allocated default vm_parameters, so C clients can use the C++
// constructor's defaults without invoking it directly.  Free with
// nf_free_parameters when done.
VM_C_API vm_parameters* nf_default_parameters(void) {
  return new vm_parameters();
}

VM_C_API void nf_free_parameters(vm_parameters* p) {
  delete p;
}

// Setters so C clients don't have to know the vm_parameters layout.
// image_path is strdup'd so the caller can free their copy any time, and
// so that ~vm_parameters can safely free() it.  (Init_factor itself does
// NOT strdup p->image_path if non-NULL — the destructor expects a
// heap-allocated pointer, so we must heap-allocate it here.)
VM_C_API void nf_params_set_image_path(vm_parameters* p, const vm_char* path) {
  if (p->image_path) free((vm_char*)p->image_path);
  p->image_path = safe_strdup(path);
}

VM_C_API void nf_params_set_signals(vm_parameters* p, bool enable) {
  p->signals = enable;
}

VM_C_API void nf_init_factor(factor_vm* vm, vm_parameters* p) {
  vm->init_factor(p);
}

VM_C_API void nf_pass_args(factor_vm* vm, int argc, vm_char** argv) {
  vm->pass_args_to_factor(argc, argv);
}

VM_C_API char* nf_eval_string(factor_vm* vm, char* s) {
  return vm->factor_eval_string(s);
}

VM_C_API void nf_eval_free(factor_vm* vm, char* p) {
  vm->factor_eval_free(p);
}

VM_C_API void nf_yield(factor_vm* vm) {
  vm->factor_yield();
}

VM_C_API void nf_sleep(factor_vm* vm, long us) {
  vm->factor_sleep(us);
}

VM_C_API void nf_stop(factor_vm* vm) {
  vm->stop_factor();
}

// Direct quotation call — bypasses the parser.  `quot` is a tagged cell
// (high bits = quotation address, low 4 bits = QUOTATION_TYPE = 4).
// Uses c_to_factor_toplevel which installs SEH function-table entries
// so Factor's exception unwinding works.
VM_C_API void nf_call_quotation(factor_vm* vm, cell quot) {
  vm->c_to_factor_toplevel(quot);
}

// Run the image's startup quotation (OBJ_STARTUP_QUOT).  Use this after
// init_factor on an image whose startup quotation has been set to e.g.
// `[ init-remote-control ]` — it will execute that quotation in the VM,
// installing the eval/yield/sleep callbacks so subsequent nf_eval_string
// calls work.
VM_C_API void nf_run_startup(factor_vm* vm) {
  vm->c_to_factor_toplevel(vm->special_objects[OBJ_STARTUP_QUOT]);
}

// Read/write a special object slot directly — for advanced setup that
// needs to inspect or replace e.g. OBJ_STARTUP_QUOT without going
// through Factor source.
VM_C_API cell nf_get_special_object(factor_vm* vm, cell idx) {
  return vm->special_objects[idx];
}

VM_C_API void nf_set_special_object(factor_vm* vm, cell idx, cell value) {
  vm->special_objects[idx] = value;
}

// Datastack access for the C/Rust host.  The data stack is the current
// context's; assumes vm->ctx is valid (set up by init_factor or by an
// active call into Factor).
VM_C_API void nf_push(factor_vm* vm, cell value) {
  vm->ctx->push(value);
}

VM_C_API cell nf_pop(factor_vm* vm) {
  return vm->ctx->pop();
}

VM_C_API cell nf_peek(factor_vm* vm) {
  return *(cell*)vm->ctx->datastack;
}

// Number of cells currently on the data stack.
VM_C_API cell nf_datastack_depth(factor_vm* vm) {
  cell start = vm->ctx->datastack_seg->start;
  cell top = vm->ctx->datastack;
  return (top + sizeof(cell) - start) / sizeof(cell);
}

// Request the currently-running Factor word to be interrupted.
// Called from a thread OTHER than the VM thread (host's watchdog,
// say) when an eval has overstayed its welcome.
//
// Mechanism: sets `stop_on_ctrl_break = true` so the safepoint
// handler raises `ERROR_INTERRUPT` instead of dropping into the
// low-level debugger (`factorbug`).  Then calls `enqueue_fep()`
// to mark the safepoint guard.  At the worker thread's next
// safepoint check (which happens at every loop back-edge and
// every word call), the worker raises `ERROR_INTERRUPT` — a
// normal Factor exception, catchable by `recover` in the
// eval-callback machinery.
//
// Effect on the worker:
//   - If the worker was inside a tight loop, it breaks out at
//     the next safepoint (microseconds latency).
//   - If the worker was idle (between evals), the FEP fires
//     on its next call into Factor and resolves immediately.
//   - The session is NOT killed — the worker thread keeps
//     running and accepts new commands.  This is the key
//     property: the user's "language thread crashes, restart"
//     promise becomes "language stalled, interrupted, continue".
VM_C_API void nf_enqueue_interrupt(factor_vm* vm) {
  vm->stop_on_ctrl_break = true;
  vm->enqueue_fep();
}

}
