// SPDX-FileCopyrightText: 2026 Spidola contributors
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// The JNI bridge to libmpv's client API, following the integration lineage of mpv-android
// (TECH_SPEC §7).
//
// One file because this is one concept — "the C surface of libmpv that Kotlin may touch" —
// and splitting it would mean exporting the handle and global-ref discipline across a
// header, which is exactly the state that must not leak.
//
// Two invariants this file exists to hold, both of which crash on every zap if broken:
//
//  1. **The Surface global ref outlives mpv's use of it.** mpv takes the Surface as the
//     `wid` option and calls ANativeWindow_fromSurface on it from its own render thread
//     (mpv video/out/android_common.c:51-52). A local ref would be dead the moment this
//     function returned; a global ref deleted while the VO still holds the window is a
//     use-after-free. So the ref is created here, and deleted only after the caller has
//     already torn the VO down — see MpvSurfaceOwner on the Kotlin side for the ordering
//     this file assumes.
//
//  2. **No mpv call outlives mpv_terminate_destroy.** The handle is owned by Kotlin as an
//     opaque long. Kotlin joins the event thread before calling nativeDestroy; this file
//     does not defend against a caller that does not, because it cannot — a stale handle is
//     indistinguishable from a live one in C.

#include <jni.h>
#include <stdlib.h>
#include <string.h>
#include <android/native_window_jni.h>
#include <libavcodec/jni.h>
#include <mpv/client.h>

#define TO_HANDLE(h) ((mpv_handle *)(intptr_t)(h))

// Cached so the event pump does not do a class lookup per event. Global refs, released in
// JNI_OnUnload.
static jclass g_event_class = NULL;
static jmethodID g_event_ctor = NULL;

// The Surface global ref backing mpv's `wid`. One per process is enough: an engine owns the
// only mpv instance that renders, and the Kotlin side serialises attach/detach.
static jobject g_surface_ref = NULL;

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) {
        return JNI_ERR;
    }

    // mpv's Android video and AudioTrack integrations retrieve the process VM through FFmpeg's
    // JNI registry. Without this, vo=gpu can open the media and still fail to create an
    // ANativeWindow with "No Java virtual machine has been registered".
    if (av_jni_set_java_vm(vm, NULL) < 0) {
        return JNI_ERR;
    }

    jclass local = (*env)->FindClass(env, "dev/spidola/tv/player/enginempv/MpvEvent");
    if (local == NULL) {
        return JNI_ERR;
    }
    g_event_class = (*env)->NewGlobalRef(env, local);
    (*env)->DeleteLocalRef(env, local);

    g_event_ctor = (*env)->GetMethodID(env, g_event_class, "<init>",
                                       "(IIILjava/lang/String;Ljava/lang/String;)V");
    if (g_event_ctor == NULL) {
        return JNI_ERR;
    }
    return JNI_VERSION_1_6;
}

JNIEXPORT void JNICALL JNI_OnUnload(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) {
        return;
    }
    if (g_event_class != NULL) {
        (*env)->DeleteGlobalRef(env, g_event_class);
        g_event_class = NULL;
    }
    if (g_surface_ref != NULL) {
        (*env)->DeleteGlobalRef(env, g_surface_ref);
        g_surface_ref = NULL;
    }
}

static jstring to_jstring(JNIEnv *env, const char *s) {
    return s == NULL ? NULL : (*env)->NewStringUTF(env, s);
}

JNIEXPORT jlong JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeCreate(JNIEnv *env, jobject thiz) {
    (void)env;
    (void)thiz;
    return (jlong)(intptr_t)mpv_create();
}

JNIEXPORT jint JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeInitialize(JNIEnv *env, jobject thiz, jlong handle) {
    (void)env;
    (void)thiz;
    return mpv_initialize(TO_HANDLE(handle));
}

JNIEXPORT void JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeDestroy(JNIEnv *env, jobject thiz, jlong handle) {
    (void)env;
    (void)thiz;
    // Terminates the core and frees the handle. Every other native entry point here reads
    // the handle, so the caller must have joined the event thread before reaching this.
    mpv_terminate_destroy(TO_HANDLE(handle));
}

JNIEXPORT jint JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeSetOptionString(
        JNIEnv *env, jobject thiz, jlong handle, jstring name, jstring value) {
    (void)thiz;
    const char *n = (*env)->GetStringUTFChars(env, name, NULL);
    const char *v = (*env)->GetStringUTFChars(env, value, NULL);
    int rc = mpv_set_option_string(TO_HANDLE(handle), n, v);
    (*env)->ReleaseStringUTFChars(env, name, n);
    (*env)->ReleaseStringUTFChars(env, value, v);
    return rc;
}

JNIEXPORT jint JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeSetPropertyString(
        JNIEnv *env, jobject thiz, jlong handle, jstring name, jstring value) {
    (void)thiz;
    const char *n = (*env)->GetStringUTFChars(env, name, NULL);
    const char *v = (*env)->GetStringUTFChars(env, value, NULL);
    int rc = mpv_set_property_string(TO_HANDLE(handle), n, v);
    (*env)->ReleaseStringUTFChars(env, name, n);
    (*env)->ReleaseStringUTFChars(env, value, v);
    return rc;
}

JNIEXPORT jstring JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeGetPropertyString(
        JNIEnv *env, jobject thiz, jlong handle, jstring name) {
    (void)thiz;
    const char *n = (*env)->GetStringUTFChars(env, name, NULL);
    char *value = mpv_get_property_string(TO_HANDLE(handle), n);
    (*env)->ReleaseStringUTFChars(env, name, n);
    if (value == NULL) {
        return NULL;
    }
    jstring result = (*env)->NewStringUTF(env, value);
    // mpv owns this allocation; NewStringUTF has already copied it.
    mpv_free(value);
    return result;
}

JNIEXPORT jint JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeCommand(
        JNIEnv *env, jobject thiz, jlong handle, jobjectArray args) {
    (void)thiz;
    jsize count = (*env)->GetArrayLength(env, args);
    // +1 for the NULL terminator mpv_command requires.
    const char **argv = calloc((size_t)count + 1, sizeof(char *));
    if (argv == NULL) {
        return MPV_ERROR_NOMEM;
    }
    jstring *locals = calloc((size_t)count, sizeof(jstring));
    if (locals == NULL) {
        free(argv);
        return MPV_ERROR_NOMEM;
    }

    for (jsize i = 0; i < count; i++) {
        locals[i] = (jstring)(*env)->GetObjectArrayElement(env, args, i);
        argv[i] = (*env)->GetStringUTFChars(env, locals[i], NULL);
    }

    int rc = mpv_command(TO_HANDLE(handle), argv);

    for (jsize i = 0; i < count; i++) {
        (*env)->ReleaseStringUTFChars(env, locals[i], argv[i]);
        (*env)->DeleteLocalRef(env, locals[i]);
    }
    free(locals);
    free(argv);
    return rc;
}

JNIEXPORT jint JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeObserveProperty(
        JNIEnv *env, jobject thiz, jlong handle, jstring name, jint format) {
    (void)thiz;
    const char *n = (*env)->GetStringUTFChars(env, name, NULL);
    int rc = mpv_observe_property(TO_HANDLE(handle), 0, n, (mpv_format)format);
    (*env)->ReleaseStringUTFChars(env, name, n);
    return rc;
}

JNIEXPORT void JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeWakeup(JNIEnv *env, jobject thiz, jlong handle) {
    (void)env;
    (void)thiz;
    // Unblocks a nativeWaitEvent in progress. This is the only mpv call that is safe from a
    // thread other than the one pumping events, and it is how release() ends the pump.
    mpv_wakeup(TO_HANDLE(handle));
}

JNIEXPORT jobject JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeWaitEvent(
        JNIEnv *env, jobject thiz, jlong handle, jdouble timeout) {
    (void)thiz;
    mpv_event *event = mpv_wait_event(TO_HANDLE(handle), timeout);
    if (event == NULL || event->event_id == MPV_EVENT_NONE) {
        return NULL;
    }

    jint end_reason = -1;
    jint end_error = 0;
    jstring name = NULL;
    jstring value = NULL;

    switch (event->event_id) {
        case MPV_EVENT_END_FILE: {
            mpv_event_end_file *ef = event->data;
            end_reason = (jint)ef->reason;
            end_error = (jint)ef->error;
            break;
        }
        case MPV_EVENT_PROPERTY_CHANGE: {
            mpv_event_property *prop = event->data;
            name = to_jstring(env, prop->name);
            // Every property this engine observes is registered as MPV_FORMAT_STRING or
            // MPV_FORMAT_NONE (see MpvClient.observeDefaults), so the only payload that can
            // arrive is a string, and a NONE observation carries no data at all. Anything
            // else would be a registration this switch has not been taught about, and is
            // reported with a null value rather than reinterpreted.
            if (prop->format == MPV_FORMAT_STRING && prop->data != NULL) {
                value = to_jstring(env, *(char **)prop->data);
            }
            break;
        }
        case MPV_EVENT_LOG_MESSAGE: {
            mpv_event_log_message *msg = event->data;
            name = to_jstring(env, msg->prefix);
            value = to_jstring(env, msg->text);
            break;
        }
        default:
            break;
    }

    jobject result = (*env)->NewObject(env, g_event_class, g_event_ctor,
                                       (jint)event->event_id, end_reason, end_error, name, value);
    if (name != NULL) {
        (*env)->DeleteLocalRef(env, name);
    }
    if (value != NULL) {
        (*env)->DeleteLocalRef(env, value);
    }
    return result;
}

JNIEXPORT jint JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeRequestLogMessages(
        JNIEnv *env, jobject thiz, jlong handle, jstring level) {
    (void)thiz;
    const char *l = (*env)->GetStringUTFChars(env, level, NULL);
    int rc = mpv_request_log_messages(TO_HANDLE(handle), l);
    (*env)->ReleaseStringUTFChars(env, level, l);
    return rc;
}

JNIEXPORT jint JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeAttachSurface(
        JNIEnv *env, jobject thiz, jlong handle, jobject surface) {
    (void)thiz;
    // Replacing an attached surface without a detach would strand the previous global ref.
    // Refuse rather than leak: the Kotlin side always detaches first, so reaching this means
    // a caller bug worth failing loudly for.
    if (g_surface_ref != NULL) {
        return MPV_ERROR_INVALID_PARAMETER;
    }

    // The ref mpv will dereference from its render thread, long after this returns.
    g_surface_ref = (*env)->NewGlobalRef(env, surface);
    if (g_surface_ref == NULL) {
        return MPV_ERROR_NOMEM;
    }

    // mpv reads `wid` as a jobject cast through intptr_t (android_common.c:51). It is an
    // option, not a property: it must be set while the VO is down, which is why the Kotlin
    // side attaches before enabling vo=gpu.
    char wid[32];
    snprintf(wid, sizeof(wid), "%" PRId64, (int64_t)(intptr_t)g_surface_ref);
    int rc = mpv_set_option_string(TO_HANDLE(handle), "wid", wid);
    if (rc < 0) {
        (*env)->DeleteGlobalRef(env, g_surface_ref);
        g_surface_ref = NULL;
    }
    return rc;
}

JNIEXPORT void JNICALL
Java_dev_spidola_tv_player_enginempv_MpvClient_nativeDetachSurface(
        JNIEnv *env, jobject thiz, jlong handle) {
    (void)thiz;
    if (g_surface_ref == NULL) {
        return;
    }
    // Ordering is the whole point. By contract the caller has already driven vo=null and
    // waited for it, so mpv's VO is down and no render thread holds the ANativeWindow. Only
    // then is clearing wid and dropping the ref safe.
    //
    // The handle may legitimately be 0 here: the surface can be destroyed after the engine
    // was released, and there is then no mpv left to tell.
    if (handle != 0) {
        mpv_set_option_string(TO_HANDLE(handle), "wid", "0");
    }
    (*env)->DeleteGlobalRef(env, g_surface_ref);
    g_surface_ref = NULL;
}
