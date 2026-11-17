/* Development host; no subprocess rendering. SPDX-License-Identifier: GPL-3.0-or-later */
#include "avid_core.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
struct events { avid_handle cancel; int cancel_on_progress; };
static void event(void *context, const uint8_t *data, size_t length) {
    struct events *events=(struct events *)context;
    if(events && events->cancel_on_progress && length>10) {
        for(size_t i=0;i+10<=length;++i) if(!memcmp(data+i,"\"progress\"",10)){avid_core_cancel(events->cancel);break;}
    }
    fwrite(data, 1, length, stderr); fputc('\n', stderr);
}
int main(int argc, char **argv) {
    if (argc != 2 && argc != 3) { fprintf(stderr, "Usage: linked-host [--cancel-on-progress] request.json\n"); return 2; }
    FILE *file = fopen(argv[argc-1], "rb"); if (!file) return 2;
    if (fseek(file,0,SEEK_END)) return 2;
    long length=ftell(file); if (length <= 0 || length > 4*1024*1024) return 2;
    rewind(file); uint8_t *request=(uint8_t *)malloc((size_t)length);
    if (!request || fread(request,1,(size_t)length,file)!=(size_t)length) return 2;
    fclose(file);
    if (avid_core_abi_version()!=1) return 2;
    avid_handle core=avid_core_create(), cancel=avid_core_cancel_create();
    struct events events={cancel,argc==3 && !strcmp(argv[1],"--cancel-on-progress")};
    avid_handle result=avid_core_execute_json(core,cancel,request,(size_t)length,event,&events);
    free(request); size_t size=avid_core_buffer_len(result); uint8_t *bytes=(uint8_t *)malloc(size+1);
    if (!bytes || !size || avid_core_buffer_copy(result,bytes,size)!=size) return 2;
    bytes[size]=0; int success=strstr((const char*)bytes,"\"ok\":true")!=NULL;
    fwrite(bytes,1,size,stdout); fputc('\n',stdout); free(bytes);
    avid_core_release(result); avid_core_release(cancel); avid_core_release(core);
    return success?0:10;
}
