#ifndef NMP_CORE_H
#define NMP_CORE_H

// The iOS app consumes Podcast runtime/domain APIs through the generated
// `PodcastApp` UniFFI binding. This bridging header is kept only for the
// host-owned local LLM callback socket.

typedef char* (*NmpLocalLlmFn)(void* context, const char* prompt_json);
void nmp_app_register_local_llm(void* handle, void* context, NmpLocalLlmFn fn);
void nmp_app_clear_local_llm(void* handle);

#endif
