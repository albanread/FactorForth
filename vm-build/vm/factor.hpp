namespace factor {

factor_vm* new_factor_vm();
VM_C_API void start_standalone_factor(int argc, vm_char** argv);

// NewFactor embedding API — see vm/factor.cpp
VM_C_API factor_vm* nf_new_vm(void);
VM_C_API vm_parameters* nf_default_parameters(void);
VM_C_API void nf_free_parameters(vm_parameters* p);
VM_C_API void nf_params_set_image_path(vm_parameters* p, const vm_char* path);
VM_C_API void nf_params_set_signals(vm_parameters* p, bool enable);
VM_C_API void nf_init_factor(factor_vm* vm, vm_parameters* p);
VM_C_API void nf_pass_args(factor_vm* vm, int argc, vm_char** argv);
VM_C_API char* nf_eval_string(factor_vm* vm, char* s);
VM_C_API void nf_eval_free(factor_vm* vm, char* p);
VM_C_API void nf_yield(factor_vm* vm);
VM_C_API void nf_sleep(factor_vm* vm, long us);
VM_C_API void nf_stop(factor_vm* vm);
VM_C_API void nf_call_quotation(factor_vm* vm, cell quot);
VM_C_API void nf_run_startup(factor_vm* vm);
VM_C_API cell nf_get_special_object(factor_vm* vm, cell idx);
VM_C_API void nf_set_special_object(factor_vm* vm, cell idx, cell value);
VM_C_API void nf_push(factor_vm* vm, cell value);
VM_C_API cell nf_pop(factor_vm* vm);
VM_C_API cell nf_peek(factor_vm* vm);
VM_C_API cell nf_datastack_depth(factor_vm* vm);

// image
bool factor_arg(const vm_char* str, const vm_char* arg, cell* value);

// objects
cell object_size(cell tagged);

// os-*
void open_console();
void close_console();
void lock_console();
void unlock_console();
bool move_file(const vm_char* path1, const vm_char* path2);

void ignore_ctrl_c();
void handle_ctrl_c();

bool set_memory_locked(cell base, cell size, bool locked);

}
