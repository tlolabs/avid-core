/* Failure-only diagnostics for a synthetic runtime directory and its two tools.
 * Owner executable identity is queried only for registered resource owners. */
#include <windows.h>
#include <restartmanager.h>
#include <stdio.h>
#include <stdlib.h>
int wmain(int argc, wchar_t **argv) {
    if(argc!=4) return 1;
    DWORD session=0,reboot=0;
    WCHAR key[CCH_RM_SESSION_KEY+1]={0};
    DWORD result=RmStartSession(&session,0,key);
    wprintf(L"RmStartSession=%lu\n",result);
    if(result) return 1;
    for(int i=1;i<argc;i++) {
        DWORD attributes=GetFileAttributesW(argv[i]);
        HANDLE file=CreateFileW(argv[i],DELETE,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,
            NULL,OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS,NULL);
        DWORD error=file==INVALID_HANDLE_VALUE?GetLastError():0;
        if(file!=INVALID_HANDLE_VALUE) CloseHandle(file);
        wprintf(L"test_resource=%d attributes=%lu delete_handle_error=%lu\n",i,attributes,error);
    }
    result=RmRegisterResources(session,2,(LPCWSTR *)&argv[2],0,NULL,0,NULL);
    wprintf(L"RmRegisterResources=%lu\n",result);
    UINT needed=0,count=0;
    if(!result) result=RmGetList(session,&needed,&count,NULL,&reboot);
    wprintf(L"RmGetList=%lu needed=%u reboot=%lu\n",result,needed,reboot);
    if(result==ERROR_MORE_DATA) {
        RM_PROCESS_INFO *processes=calloc(needed,sizeof(*processes));
        count=needed;
        result=RmGetList(session,&needed,&count,processes,&reboot);
        wprintf(L"RmGetList populated=%lu count=%u\n",result,count);
        if(!result) for(UINT i=0;i<count;i++) {
            RM_PROCESS_INFO *p=&processes[i];
            HANDLE process=OpenProcess(SYNCHRONIZE|PROCESS_QUERY_LIMITED_INFORMATION,FALSE,p->Process.dwProcessId);
            DWORD wait=process?WaitForSingleObject(process,0):GetLastError();
            wprintf(L"resource_owner_pid=%lu type=%u status=%lu wait=%lu\n",
                p->Process.dwProcessId,p->ApplicationType,p->AppStatus,wait);
            if(process) {
                WCHAR path[32768]; DWORD length=32768,exitcode=0;
                if(QueryFullProcessImageNameW(process,0,path,&length)) {
                    WCHAR *name=wcsrchr(path,L'\\'); name=name?name+1:path;
                    for(WCHAR *c=name;*c;c++) if(!((*c>=L'a'&&*c<=L'z')||(*c>=L'A'&&*c<=L'Z')||(*c>=L'0'&&*c<=L'9')||*c==L'.'||*c==L'_'||*c==L'-')) *c=L'_';
                    wprintf(L"resource_owner_name=%ls\n",name);
                }
                if(GetExitCodeProcess(process,&exitcode)) wprintf(L"resource_owner_exit=%lu\n",exitcode);
                CloseHandle(process);
            }
        }
        free(processes);
    }
    RmEndSession(session);
    return 0;
}
