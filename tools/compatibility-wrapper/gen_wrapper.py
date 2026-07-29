import re
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]

native_transforms = (ROOT / 'crates/mh3g-save-convert/src/transforms.rs').read_text(encoding='utf-8')
native_markers = (
    'const SHAKALAKA_U16_TABLE_END: usize = 0xE6;',
    'const GUILD_CARD_ARENA_RECORD_COUNT: usize = 110;',
    'fn apply_shakalaka_companion_corrections(',
    'fn apply_guild_card_arena_corrections(',
)
if all(marker in native_transforms for marker in native_markers):
    raise SystemExit(
        'legacy compatibility wrapper disabled: the Rust core already contains '
        'the complete arena and Shakalaka conversion'
    )
src = (ROOT / 'crates/mh3g-save-convert/src/meow_transform_table.rs').read_text(encoding='utf-8')

def arr(name):
    m=re.search(rf'pub const {name}: \[usize; \d+\] = \[(.*?)\];',src,re.S)
    if not m: raise SystemExit(name)
    return [int(x) for x in re.findall(r'\b\d+\b',m.group(1))]
card=arr('MEOW_CARD_ARENA4')
keys=[]
for off in card:
    slot=off//0xE00
    rel=off%0xE00
    assert rel>=0x9B4 and (rel-0x9B4)%4==0
    row=(rel-0x9B4)//4
    assert row<110 and slot<98
    keys.append(slot*110+row)
assert len(keys)==382 and len(set(keys))==382
cec_rows=sorted(k for k in keys if k<110)
print('keys',len(keys),'cec',len(cec_rows),cec_rows)
arr_text=', '.join(map(str,sorted(keys)))
# format 16 per line
vals=sorted(keys)
lines=[]
for i in range(0,len(vals),16): lines.append('    '+', '.join(map(str,vals[i:i+16]))+',')
keys_c='\n'.join(lines)

c = r'''#define WIN32_LEAN_AND_MEAN

typedef unsigned char BYTE;
typedef unsigned short WORD;
typedef unsigned long DWORD;
typedef long LONG;
typedef int BOOL;
typedef unsigned long long ULONGLONG;
typedef long long LONGLONG;
typedef unsigned long long SIZE_T;
typedef __WCHAR_TYPE__ WCHAR;
typedef WCHAR *LPWSTR;
typedef const WCHAR *LPCWSTR;
typedef void *HANDLE;
typedef void *LPVOID;
typedef const void *LPCVOID;
typedef DWORD *LPDWORD;
typedef struct { DWORD dwLowDateTime; DWORD dwHighDateTime; } FILETIME;
typedef union { struct { DWORD LowPart; LONG HighPart; }; LONGLONG QuadPart; } LARGE_INTEGER;
typedef struct {
    DWORD dwFileAttributes;
    FILETIME ftCreationTime;
    FILETIME ftLastAccessTime;
    FILETIME ftLastWriteTime;
    DWORD nFileSizeHigh;
    DWORD nFileSizeLow;
    DWORD dwReserved0;
    DWORD dwReserved1;
    WCHAR cFileName[260];
    WCHAR cAlternateFileName[14];
} WIN32_FIND_DATAW;
typedef struct {
    DWORD cb;
    LPWSTR lpReserved;
    LPWSTR lpDesktop;
    LPWSTR lpTitle;
    DWORD dwX;
    DWORD dwY;
    DWORD dwXSize;
    DWORD dwYSize;
    DWORD dwXCountChars;
    DWORD dwYCountChars;
    DWORD dwFillAttribute;
    DWORD dwFlags;
    WORD wShowWindow;
    WORD cbReserved2;
    BYTE *lpReserved2;
    HANDLE hStdInput;
    HANDLE hStdOutput;
    HANDLE hStdError;
} STARTUPINFOW;
typedef struct { HANDLE hProcess; HANDLE hThread; DWORD dwProcessId; DWORD dwThreadId; } PROCESS_INFORMATION;

#define DLLIMPORT __declspec(dllimport)
#define WINAPI __stdcall
#define TRUE 1
#define FALSE 0
#define INVALID_HANDLE_VALUE ((HANDLE)(~(SIZE_T)0))
#define MAX_PATH 260
#define FILE_ATTRIBUTE_DIRECTORY 0x10
#define FILE_ATTRIBUTE_NORMAL 0x80
#define GENERIC_READ 0x80000000UL
#define GENERIC_WRITE 0x40000000UL
#define FILE_SHARE_READ 0x00000001UL
#define FILE_SHARE_WRITE 0x00000002UL
#define OPEN_EXISTING 3
#define CREATE_ALWAYS 2
#define STD_INPUT_HANDLE ((DWORD)-10)
#define STD_OUTPUT_HANDLE ((DWORD)-11)
#define STD_ERROR_HANDLE ((DWORD)-12)
#define STARTF_USESTDHANDLES 0x00000100
#define INFINITE 0xFFFFFFFFUL
#define HEAP_ZERO_MEMORY 0x00000008UL
#define MOVEFILE_REPLACE_EXISTING 0x00000001UL

DLLIMPORT void WINAPI ExitProcess(DWORD);
DLLIMPORT LPWSTR WINAPI GetCommandLineW(void);
DLLIMPORT BOOL WINAPI CreateProcessW(LPCWSTR, LPWSTR, LPVOID, LPVOID, BOOL, DWORD, LPVOID, LPCWSTR, STARTUPINFOW*, PROCESS_INFORMATION*);
DLLIMPORT DWORD WINAPI WaitForSingleObject(HANDLE, DWORD);
DLLIMPORT BOOL WINAPI GetExitCodeProcess(HANDLE, LPDWORD);
DLLIMPORT BOOL WINAPI CloseHandle(HANDLE);
DLLIMPORT DWORD WINAPI GetModuleFileNameW(HANDLE, LPWSTR, DWORD);
DLLIMPORT HANDLE WINAPI CreateFileW(LPCWSTR, DWORD, DWORD, LPVOID, DWORD, DWORD, HANDLE);
DLLIMPORT BOOL WINAPI ReadFile(HANDLE, LPVOID, DWORD, LPDWORD, LPVOID);
DLLIMPORT BOOL WINAPI WriteFile(HANDLE, LPCVOID, DWORD, LPDWORD, LPVOID);
DLLIMPORT BOOL WINAPI GetFileSizeEx(HANDLE, LARGE_INTEGER*);
DLLIMPORT HANDLE WINAPI GetStdHandle(DWORD);
DLLIMPORT DWORD WINAPI GetTempPathW(DWORD, LPWSTR);
DLLIMPORT DWORD WINAPI GetCurrentProcessId(void);
DLLIMPORT DWORD WINAPI GetTickCount(void);
DLLIMPORT BOOL WINAPI CreateDirectoryW(LPCWSTR, LPVOID);
DLLIMPORT BOOL WINAPI RemoveDirectoryW(LPCWSTR);
DLLIMPORT BOOL WINAPI DeleteFileW(LPCWSTR);
DLLIMPORT BOOL WINAPI CopyFileW(LPCWSTR, LPCWSTR, BOOL);
DLLIMPORT HANDLE WINAPI FindFirstFileW(LPCWSTR, WIN32_FIND_DATAW*);
DLLIMPORT BOOL WINAPI FindNextFileW(HANDLE, WIN32_FIND_DATAW*);
DLLIMPORT BOOL WINAPI FindClose(HANDLE);
DLLIMPORT HANDLE WINAPI GetProcessHeap(void);
DLLIMPORT LPVOID WINAPI HeapAlloc(HANDLE, DWORD, SIZE_T);
DLLIMPORT BOOL WINAPI HeapFree(HANDLE, DWORD, LPVOID);
DLLIMPORT LPWSTR* WINAPI CommandLineToArgvW(LPCWSTR, int*);
DLLIMPORT HANDLE WINAPI LocalFree(HANDLE);

void *memset(void *dst, int c, SIZE_T n) { BYTE *p=(BYTE*)dst; while(n--) *p++=(BYTE)c; return dst; }
void *memcpy(void *dst, const void *src, SIZE_T n) { BYTE *d=(BYTE*)dst; const BYTE *s=(const BYTE*)src; while(n--) *d++=*s++; return dst; }

static SIZE_T wslen(LPCWSTR s){ SIZE_T n=0; if(s) while(s[n]) n++; return n; }
static BOOL wseq(LPCWSTR a,LPCWSTR b){ SIZE_T i=0; if(!a||!b) return FALSE; while(a[i]&&b[i]&&a[i]==b[i]) i++; return a[i]==b[i]; }
static void wcopy(LPWSTR d,LPCWSTR s){ while((*d++=*s++)); }
static void wcat(LPWSTR d,LPCWSTR s){ d+=wslen(d); wcopy(d,s); }
static BOOL is_dot(LPCWSTR s){ return (s[0]==L'.'&&s[1]==0)||(s[0]==L'.'&&s[1]==L'.'&&s[2]==0); }
static void u32_to_w(DWORD v,LPWSTR out){ WCHAR tmp[16]; int n=0,i; if(v==0){out[0]=L'0';out[1]=0;return;} while(v){tmp[n++]=(WCHAR)(L'0'+v%10);v/=10;} for(i=0;i<n;i++)out[i]=tmp[n-1-i];out[n]=0; }
static void stderr_ascii(const char *s){ DWORD n=0,w=0; while(s[n])n++; WriteFile(GetStdHandle(STD_ERROR_HANDLE),s,n,&w,0); }

static BOOL path_join(LPWSTR out,SIZE_T cap,LPCWSTR a,LPCWSTR b){ SIZE_T na=wslen(a),nb=wslen(b); BOOL slash=na>0&&a[na-1]!=L'\\'&&a[na-1]!=L'/'; if(na+nb+(slash?1:0)+1>cap)return FALSE; wcopy(out,a); if(slash)out[na++]=L'\\'; wcopy(out+na,b); return TRUE; }
static void dirname_inplace(LPWSTR p){ SIZE_T n=wslen(p); while(n>0){ if(p[n-1]==L'\\'||p[n-1]==L'/'){p[n-1]=0;return;} n--; } p[0]=0; }

static BOOL read_all(LPCWSTR path,BYTE **buf,SIZE_T *sz){
    HANDLE h=CreateFileW(path,GENERIC_READ,FILE_SHARE_READ,0,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,0); LARGE_INTEGER li; DWORD got=0,total=0; BYTE *p;
    if(h==INVALID_HANDLE_VALUE)return FALSE; if(!GetFileSizeEx(h,&li)||li.QuadPart<0||li.QuadPart>0x7fffffff){CloseHandle(h);return FALSE;}
    *sz=(SIZE_T)li.QuadPart; p=(BYTE*)HeapAlloc(GetProcessHeap(),0,*sz?*sz:1); if(!p){CloseHandle(h);return FALSE;}
    while(total<(DWORD)*sz){ DWORD want=(DWORD)*sz-total; if(!ReadFile(h,p+total,want,&got,0)||got==0){HeapFree(GetProcessHeap(),0,p);CloseHandle(h);return FALSE;} total+=got; }
    CloseHandle(h); *buf=p; return TRUE;
}
static BOOL write_all(LPCWSTR path,const BYTE *buf,SIZE_T sz){
    HANDLE h=CreateFileW(path,GENERIC_WRITE,0,0,CREATE_ALWAYS,FILE_ATTRIBUTE_NORMAL,0); DWORD put=0,total=0; if(h==INVALID_HANDLE_VALUE)return FALSE;
    while(total<(DWORD)sz){ DWORD want=(DWORD)sz-total; if(!WriteFile(h,buf+total,want,&put,0)||put==0){CloseHandle(h);return FALSE;} total+=put; }
    CloseHandle(h); return TRUE;
}
static void swap2(BYTE *p){ BYTE t=p[0];p[0]=p[1];p[1]=t; }
static void arena4(BYTE *p){ DWORD v=(DWORD)p[0]|((DWORD)p[1]<<8)|((DWORD)p[2]<<16)|((DWORD)p[3]<<24); v=(v<<17)|(v>>15); p[0]=(BYTE)(v>>24);p[1]=(BYTE)(v>>16);p[2]=(BYTE)(v>>8);p[3]=(BYTE)v; }
static BOOL old_shaka_u16(int companion,int rel){
    static const WORD c0[]={0x0C,0x0E,0x10,0x14,0x18,0x1C};
    static const WORD c1[]={0x0C,0x0E,0x12,0x18,0x1C,0x24,0x2C};
    const WORD *a=companion?c1:c0; int n=companion?7:6,i; for(i=0;i<n;i++)if(a[i]==rel)return TRUE; return FALSE;
}
static BOOL patch_user_file(LPCWSTR path){ BYTE *b; SIZE_T sz; int c,rel; if(!read_all(path,&b,&sz))return FALSE; if(sz<4+0x8A00||b[0]!=0x2B||b[1]||b[2]||b[3]){HeapFree(GetProcessHeap(),0,b);return FALSE;}
    for(c=0;c<2;c++)for(rel=0x0C;rel<0xE6;rel+=2)if(!old_shaka_u16(c,rel))swap2(b+4+0x6F44+c*0x148+rel);
    if(!write_all(path,b,sz)){HeapFree(GetProcessHeap(),0,b);return FALSE;} HeapFree(GetProcessHeap(),0,b); return TRUE; }

static const WORD CARD_STATIC_KEYS[382]={
__CARD_KEYS__
};
static BOOL old_card_key(WORD k){ int lo=0,hi=381; while(lo<=hi){int m=(lo+hi)/2;WORD v=CARD_STATIC_KEYS[m];if(v==k)return TRUE;if(v<k)lo=m+1;else hi=m-1;}return FALSE; }
static BOOL old_cec_row(int row){ return row<=63||row==65||row==70||row==75||row==76||row==95||row==105||row==106||row==107||row==108; }
static BOOL patch_card_file(LPCWSTR path){ BYTE *b;SIZE_T sz;int slot,row; if(!read_all(path,&b,&sz))return FALSE; if(sz<4+0x57FFC||b[0]!=0x2B||b[1]||b[2]||b[3]){HeapFree(GetProcessHeap(),0,b);return FALSE;}
    for(slot=0;slot<98;slot++)for(row=0;row<110;row++){WORD key=(WORD)(slot*110+row);if(!old_card_key(key))arena4(b+4+slot*0xE00+0x9B4+row*4);}
    if(!write_all(path,b,sz)){HeapFree(GetProcessHeap(),0,b);return FALSE;} HeapFree(GetProcessHeap(),0,b);return TRUE; }
static DWORD le32(const BYTE *p){return (DWORD)p[0]|((DWORD)p[1]<<8)|((DWORD)p[2]<<16)|((DWORD)p[3]<<24);}
static BOOL patch_cec_message(LPCWSTR path){BYTE*b;SIZE_T sz;DWORD hs,bs;SIZE_T rec;int slot,row;if(!read_all(path,&b,&sz))return FALSE;
    if(sz<0x70||b[0]!=0x60||b[1]!=0x60){HeapFree(GetProcessHeap(),0,b);return TRUE;} hs=le32(b+8);bs=le32(b+12);
    if(hs<0x70||bs<8+0x2A00||((SIZE_T)hs+bs)>sz||le32(b+16)!=0x00048100||le32(b+20)!=0){HeapFree(GetProcessHeap(),0,b);return TRUE;}
    rec=(SIZE_T)hs+8; if(rec+3*0xE00>sz){HeapFree(GetProcessHeap(),0,b);return TRUE;}
    for(slot=0;slot<3;slot++)for(row=0;row<110;row++)if(!old_cec_row(row))arena4(b+rec+slot*0xE00+0x9B4+row*4);
    if(!write_all(path,b,sz)){HeapFree(GetProcessHeap(),0,b);return FALSE;}HeapFree(GetProcessHeap(),0,b);return TRUE;}

static BOOL copy_tree(LPCWSTR src,LPCWSTR dst){WCHAR pat[MAX_PATH],sp[MAX_PATH],dp[MAX_PATH];WIN32_FIND_DATAW fd;HANDLE h;CreateDirectoryW(dst,0);if(!path_join(pat,MAX_PATH,src,L"*"))return FALSE;h=FindFirstFileW(pat,&fd);if(h==INVALID_HANDLE_VALUE)return FALSE;do{if(is_dot(fd.cFileName))continue;if(!path_join(sp,MAX_PATH,src,fd.cFileName)||!path_join(dp,MAX_PATH,dst,fd.cFileName)){FindClose(h);return FALSE;}if(fd.dwFileAttributes&FILE_ATTRIBUTE_DIRECTORY){if(!copy_tree(sp,dp)){FindClose(h);return FALSE;}}else if(!CopyFileW(sp,dp,FALSE)){FindClose(h);return FALSE;}}while(FindNextFileW(h,&fd));FindClose(h);return TRUE;}
static void delete_tree(LPCWSTR root){WCHAR pat[MAX_PATH],p[MAX_PATH];WIN32_FIND_DATAW fd;HANDLE h;if(!path_join(pat,MAX_PATH,root,L"*"))return;h=FindFirstFileW(pat,&fd);if(h!=INVALID_HANDLE_VALUE){do{if(is_dot(fd.cFileName))continue;if(!path_join(p,MAX_PATH,root,fd.cFileName))continue;if(fd.dwFileAttributes&FILE_ATTRIBUTE_DIRECTORY)delete_tree(p);else DeleteFileW(p);}while(FindNextFileW(h,&fd));FindClose(h);}RemoveDirectoryW(root);}
static BOOL patch_extras_dir(LPCWSTR dir){WCHAR p[MAX_PATH];int i;const WCHAR*names[3]={L"card1",L"card2",L"card3"};for(i=0;i<3;i++){if(!path_join(p,MAX_PATH,dir,names[i])||!patch_card_file(p))return FALSE;}return TRUE;}
static BOOL patch_cec_dir(LPCWSTR dir){WCHAR inbox[MAX_PATH],pat[MAX_PATH],p[MAX_PATH];WIN32_FIND_DATAW fd;HANDLE h;if(!path_join(inbox,MAX_PATH,dir,L"InBox___"))return FALSE;if(!path_join(pat,MAX_PATH,inbox,L"_*"))return FALSE;h=FindFirstFileW(pat,&fd);if(h==INVALID_HANDLE_VALUE)return TRUE;do{if(is_dot(fd.cFileName)||(fd.dwFileAttributes&FILE_ATTRIBUTE_DIRECTORY))continue;if(!path_join(p,MAX_PATH,inbox,fd.cFileName)||!patch_cec_message(p)){FindClose(h);return FALSE;}}while(FindNextFileW(h,&fd));FindClose(h);return TRUE;}

static SIZE_T quote_arg(LPWSTR out,LPCWSTR s){SIZE_T n=0,bs=0,i;out[n++]=L'"';for(i=0;;i++){WCHAR ch=s[i];if(ch==L'\\'){bs++;continue;}if(ch==L'"'){while(bs--){out[n++]=L'\\';out[n++]=L'\\';}out[n++]=L'\\';out[n++]=L'"';bs=0;continue;}if(ch==0){while(bs--){out[n++]=L'\\';out[n++]=L'\\';}break;}while(bs--)out[n++]=L'\\';bs=0;out[n++]=ch;}out[n++]=L'"';out[n]=0;return n;}
static LPWSTR build_cmd(int argc,LPWSTR*argv,LPCWSTR core,int repl_idx,LPCWSTR repl){SIZE_T cap=wslen(core)*2+64,n=0,i;LPWSTR b;for(i=1;i<(SIZE_T)argc;i++)cap+=(wslen((int)i==repl_idx?repl:argv[i])*2+4);b=(LPWSTR)HeapAlloc(GetProcessHeap(),HEAP_ZERO_MEMORY,(cap+1)*sizeof(WCHAR));if(!b)return 0;n+=quote_arg(b+n,core);for(i=1;i<(SIZE_T)argc;i++){b[n++]=L' ';n+=quote_arg(b+n,(int)i==repl_idx?repl:argv[i]);}return b;}
static DWORD run_core(LPCWSTR core,LPWSTR cmd){STARTUPINFOW si;PROCESS_INFORMATION pi;DWORD ec=2;memset(&si,0,sizeof(si));memset(&pi,0,sizeof(pi));si.cb=sizeof(si);si.dwFlags=STARTF_USESTDHANDLES;si.hStdInput=GetStdHandle(STD_INPUT_HANDLE);si.hStdOutput=GetStdHandle(STD_OUTPUT_HANDLE);si.hStdError=GetStdHandle(STD_ERROR_HANDLE);if(!CreateProcessW(core,cmd,0,0,TRUE,0,0,0,&si,&pi)){stderr_ascii("Unable to launch mh3g-save-convert-core.exe\r\n");return 2;}WaitForSingleObject(pi.hProcess,INFINITE);GetExitCodeProcess(pi.hProcess,&ec);CloseHandle(pi.hThread);CloseHandle(pi.hProcess);return ec;}

void wmainCRTStartup(void){int argc=0,i,repl=-1;LPWSTR*argv=CommandLineToArgvW(GetCommandLineW(),&argc);WCHAR module[MAX_PATH],core[MAX_PATH],tmpbase[MAX_PATH],tmproot[MAX_PATH],tmpobj[MAX_PATH],num[16];LPWSTR cmd=0;DWORD ec=2;BOOL made=FALSE,ok=TRUE;LPCWSTR replacement=0;
    if(!argv||argc<1){stderr_ascii("Invalid command line\r\n");ExitProcess(2);}if(!GetModuleFileNameW(0,module,MAX_PATH)){stderr_ascii("Cannot locate wrapper executable\r\n");goto done;}dirname_inplace(module);if(!path_join(core,MAX_PATH,module,L"mh3g-save-convert-core.exe")){stderr_ascii("Core path is too long\r\n");goto done;}
    if(argc>=3&&wseq(argv[1],L"convert")){repl=2;if(!GetTempPathW(MAX_PATH,tmpbase)){stderr_ascii("Cannot get temp directory\r\n");goto done;}wcopy(tmproot,tmpbase);wcat(tmproot,L"mh3g-fix-");u32_to_w(GetCurrentProcessId(),num);wcat(tmproot,num);wcat(tmproot,L"-");u32_to_w(GetTickCount(),num);wcat(tmproot,num);if(!CreateDirectoryW(tmproot,0)){stderr_ascii("Cannot create temp directory\r\n");goto done;}made=TRUE;if(!path_join(tmpobj,MAX_PATH,tmproot,L"source.bin")||!CopyFileW(argv[2],tmpobj,FALSE)||!patch_user_file(tmpobj)){stderr_ascii("Unable to prepare patched user save\r\n");goto done;}replacement=tmpobj;
    }else if(argc>=2&&(wseq(argv[1],L"convert-extras")||wseq(argv[1],L"convert-cec"))){for(i=2;i+1<argc;i++)if(wseq(argv[i],L"--source-dir")){repl=i+1;break;}if(repl<0){stderr_ascii("Missing --source-dir argument\r\n");goto done;}if(!GetTempPathW(MAX_PATH,tmpbase)){stderr_ascii("Cannot get temp directory\r\n");goto done;}wcopy(tmproot,tmpbase);wcat(tmproot,L"mh3g-fix-");u32_to_w(GetCurrentProcessId(),num);wcat(tmproot,num);wcat(tmproot,L"-");u32_to_w(GetTickCount(),num);wcat(tmproot,num);if(!CreateDirectoryW(tmproot,0)){stderr_ascii("Cannot create temp directory\r\n");goto done;}made=TRUE;if(!path_join(tmpobj,MAX_PATH,tmproot,L"source")||!copy_tree(argv[repl],tmpobj)){stderr_ascii("Unable to copy source directory\r\n");goto done;}if(wseq(argv[1],L"convert-extras"))ok=patch_extras_dir(tmpobj);else ok=patch_cec_dir(tmpobj);if(!ok){stderr_ascii("Unable to apply compatibility patch\r\n");goto done;}replacement=tmpobj;}
    cmd=build_cmd(argc,argv,core,repl,replacement);if(!cmd){stderr_ascii("Cannot allocate command line\r\n");goto done;}ec=run_core(core,cmd);
done: if(cmd)HeapFree(GetProcessHeap(),0,cmd);if(made)delete_tree(tmproot);if(argv)LocalFree(argv);ExitProcess(ec);}
'''.replace('__CARD_KEYS__',keys_c)
Path(__file__).with_name('wrapper.c').write_text(c, encoding='utf-8')
