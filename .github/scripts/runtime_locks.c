/* Read-only owner diagnostics for the two synthetic test executables only.
 * No process enumeration, command lines, system paths, or machine-wide trace. */
#include <windows.h>
#include <restartmanager.h>
#include <stdio.h>
#include <stdlib.h>
int wmain(int argc, wchar_t **argv) {
    if(argc!=3) return 1;
    DWORD session=0,reboot=0;
    WCHAR key[CCH_RM_SESSION_KEY+1]={0};
    DWORD result=RmStartSession(&session,0,key);
    wprintf(L"RmStartSession=%lu\n",result);
    if(result) return 1;
    for(int i=1;i<argc;i++) {
        DWORD attributes=GetFileAttributesW(argv[i]);
        HANDLE file=CreateFileW(argv[i],DELETE,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,
            NULL,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,NULL);
        DWORD error=file==INVALID_HANDLE_VALUE?GetLastError():0;
        if(file!=INVALID_HANDLE_VALUE) CloseHandle(file);
        wprintf(L"test_executable=%d attributes=%lu delete_handle_error=%lu\n",i,attributes,error);
    }
    result=RmRegisterResources(session,2,(LPCWSTR *)&argv[1],0,NULL,0,NULL);
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
            HANDLE process=OpenProcess(SYNCHRONIZE,FALSE,p->Process.dwProcessId);
            DWORD wait=process?WaitForSingleObject(process,0):GetLastError();
            wprintf(L"resource_owner_pid=%lu application=%ls type=%u status=%lu wait=%lu\n",
                p->Process.dwProcessId,p->strAppName,p->ApplicationType,p->AppStatus,wait);
            if(process) CloseHandle(process);
        }
        free(processes);
    }
    RmEndSession(session);
    return 0;
}
