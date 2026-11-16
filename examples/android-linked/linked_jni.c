/* JNI transport only; Core owns validation and media policy. */
#include <jni.h>
#include <stdlib.h>
#include "avid_core.h"
struct callback { JNIEnv *env; jobject host; jmethodID event; int failed; };
static void deliver(void *context,const uint8_t *data,size_t length) {
    struct callback *c=context;
    if (c->failed || length>2147483647) return;
    jbyteArray bytes=(*c->env)->NewByteArray(c->env,(jsize)length);
    if (!bytes) {c->failed=1;return;}
    (*c->env)->SetByteArrayRegion(c->env,bytes,0,(jsize)length,(const jbyte*)data);
    if (!(*c->env)->ExceptionCheck(c->env)) (*c->env)->CallVoidMethod(c->env,c->host,c->event,bytes);
    if ((*c->env)->ExceptionCheck(c->env)) c->failed=1;
    (*c->env)->DeleteLocalRef(c->env,bytes);
}
JNIEXPORT jlong JNICALL Java_org_avidcore_LinkedCore_create(JNIEnv *env,jclass type) {(void)env;(void)type;return (jlong)avid_core_create();}
JNIEXPORT jlong JNICALL Java_org_avidcore_LinkedCore_createCancellation(JNIEnv *env,jclass type) {(void)env;(void)type;return (jlong)avid_core_cancel_create();}
JNIEXPORT void JNICALL Java_org_avidcore_LinkedCore_cancel(JNIEnv *env,jclass type,jlong token) {(void)env;(void)type;avid_core_cancel((uint64_t)token);}
JNIEXPORT void JNICALL Java_org_avidcore_LinkedCore_release(JNIEnv *env,jclass type,jlong handle) {(void)env;(void)type;avid_core_release((uint64_t)handle);}
JNIEXPORT jbyteArray JNICALL Java_org_avidcore_LinkedCore_execute(JNIEnv *env,jobject host,jlong core,jlong token,jbyteArray request) {
    if (!request) return NULL;
    jsize length=(*env)->GetArrayLength(env,request); if(length<=0||length>4*1024*1024)return NULL;
    uint8_t *input=malloc((size_t)length);if(!input)return NULL;
    (*env)->GetByteArrayRegion(env,request,0,length,(jbyte*)input);
    if((*env)->ExceptionCheck(env)){free(input);return NULL;}
    jclass cls=(*env)->GetObjectClass(env,host);
    if(!cls){free(input);return NULL;}
    jmethodID event=(*env)->GetMethodID(env,cls,"onEvent","([B)V");
    (*env)->DeleteLocalRef(env,cls);
    if(!event){free(input);return NULL;}
    struct callback cb={env,host,event,0};
    avid_handle result=avid_core_execute_json((uint64_t)core,(uint64_t)token,input,(size_t)length,deliver,&cb);
    free(input);
    if(cb.failed||(*env)->ExceptionCheck(env)){avid_core_release(result);return NULL;}
    size_t size=avid_core_buffer_len(result);
    if(!size||size>2147483647){avid_core_release(result);return NULL;}
    uint8_t *bytes=malloc(size);if(!bytes){avid_core_release(result);return NULL;}
    if(avid_core_buffer_copy(result,bytes,size)!=size){free(bytes);avid_core_release(result);return NULL;}
    avid_core_release(result);
    jbyteArray output=(*env)->NewByteArray(env,(jsize)size);
    if(output)(*env)->SetByteArrayRegion(env,output,0,(jsize)size,(const jbyte*)bytes);
    free(bytes);return output;
}
