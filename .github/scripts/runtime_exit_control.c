/* Controlled experiment: do retained handles to an exited child prevent
 * directory replacement? Only synthetic copied FFmpeg paths are touched.
 * This is diagnostic code, never linked into Core or its runtime artifacts. */
#include <windows.h>
#include <stdio.h>
#include <wchar.h>

int wmain(int argc, wchar_t **argv) {
    if(argc != 3) return 2; /* ffmpeg.exe, fresh experiment root */
    wchar_t installed[32768], moved[32768], executable[32768], command[32768];
    swprintf(installed,32768,L"%ls\\installed",argv[2]);
    swprintf(moved,32768,L"%ls\\moved",argv[2]);
    swprintf(executable,32768,L"%ls\\ffmpeg.exe",installed);
    if(!CreateDirectoryW(argv[2],NULL) || !CreateDirectoryW(installed,NULL) ||
        !CopyFileW(argv[1],executable,TRUE)) { printf("setup error=%lu\n",GetLastError()); return 2; }
    int failures=0;
    for(int mode=0;mode<3;mode++) for(int iteration=0;iteration<200;iteration++) {
        /* 0: ordinary exit, 1: immediate termination, 2: suspended before entry. */
        swprintf(command,32768,L"\"%ls\" -v quiet -version",executable);
        STARTUPINFOW startup={.cb=sizeof(startup)};
        PROCESS_INFORMATION child={0};
        if(!CreateProcessW(executable,command,NULL,NULL,FALSE,
                CREATE_NO_WINDOW|(mode==2?CREATE_SUSPENDED:0),NULL,NULL,&startup,&child)) {
            printf("spawn error=%lu\n",GetLastError()); return 2;
        }
        if(mode && !TerminateProcess(child.hProcess,1)) {
            printf("terminate error=%lu\n",GetLastError()); return 2;
        }
        DWORD wait=WaitForSingleObject(child.hProcess,10000),code=0;
        if(wait!=WAIT_OBJECT_0 || !GetExitCodeProcess(child.hProcess,&code)) {
            printf("wait error=%lu result=%lu\n",GetLastError(),wait); return 2;
        }
        BOOL open_ok=MoveFileExW(installed,moved,0); DWORD open_error=open_ok?0:GetLastError();
        if(open_ok && !MoveFileExW(moved,installed,0)) return 2;
        CloseHandle(child.hThread); CloseHandle(child.hProcess);
        BOOL closed_ok=MoveFileExW(installed,moved,0); DWORD closed_error=closed_ok?0:GetLastError();
        printf("mode=%d iteration=%d exit=%lu waited=1 open_handles_rename=%lu closed_handles_rename=%lu\n",
            mode,iteration,code,open_error,closed_error);
        fflush(stdout);
        if(closed_ok && !MoveFileExW(moved,installed,0)) return 2;
        if(!closed_ok) { failures++; break; } /* Never retry a failed operation into success. */
    }
    if(!DeleteFileW(executable) || !RemoveDirectoryW(installed) || !RemoveDirectoryW(argv[2])) {
        printf("cleanup error=%lu\n",GetLastError()); failures++;
    }
    return failures?1:0;
}
