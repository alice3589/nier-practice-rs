//! 外部プロセスのメモリ読み書き (Cheat Engine の readFloat / writeFloat / aobscanmodule 相当)

use std::ffi::c_void;
use std::mem::{size_of, MaybeUninit};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    FlushInstructionCache, ReadProcessMemory, WriteProcessMemory,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW,
    MODULEENTRY32W, PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Memory::{VirtualProtectEx, PAGE_EXECUTE_READWRITE};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_OPERATION,
    PROCESS_VM_READ, PROCESS_VM_WRITE,
};

const STILL_ACTIVE: u32 = 259;

/// 接続中のゲームプロセス
pub struct Process {
    handle: HANDLE,
    pub pid: u32,
    /// メインモジュール (exe) のベースアドレス
    pub base: usize,
    /// メインモジュールのサイズ
    pub size: usize,
}

// HANDLE は生ポインタだが、プロセスハンドル自体はスレッド間で共有して問題ない
unsafe impl Send for Process {}
unsafe impl Sync for Process {}

fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// 実行ファイル名からプロセス ID を探す
pub fn find_pid(exe_name: &str) -> Option<u32> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                if wide_to_string(&entry.szExeFile).eq_ignore_ascii_case(exe_name) {
                    found = Some(entry.th32ProcessID);
                    break;
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
}

/// プロセス内のモジュールのベースアドレスとサイズを探す
fn find_module(pid: u32, module_name: &str) -> Option<(usize, usize)> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: MODULEENTRY32W = std::mem::zeroed();
        entry.dwSize = size_of::<MODULEENTRY32W>() as u32;
        let mut found = None;
        if Module32FirstW(snap, &mut entry) != 0 {
            loop {
                if wide_to_string(&entry.szModule).eq_ignore_ascii_case(module_name) {
                    found = Some((entry.modBaseAddr as usize, entry.modBaseSize as usize));
                    break;
                }
                if Module32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        found
    }
}

impl Process {
    /// 実行ファイル名を指定してプロセスに接続する
    pub fn attach(exe_name: &str) -> Option<Self> {
        let pid = find_pid(exe_name)?;
        let (base, size) = find_module(pid, exe_name)?;
        let access = PROCESS_VM_READ
            | PROCESS_VM_WRITE
            | PROCESS_VM_OPERATION
            | PROCESS_QUERY_LIMITED_INFORMATION;
        let handle = unsafe { OpenProcess(access, 0, pid) };
        if handle.is_null() {
            return None;
        }
        Some(Self { handle, pid, base, size })
    }

    /// プロセスがまだ生きているか
    pub fn is_alive(&self) -> bool {
        let mut code = 0u32;
        unsafe { GetExitCodeProcess(self.handle, &mut code) != 0 && code == STILL_ACTIVE }
    }

    pub fn read_into(&self, addr: usize, buf: &mut [u8]) -> bool {
        let mut read = 0usize;
        let ok = unsafe {
            ReadProcessMemory(
                self.handle,
                addr as *const c_void,
                buf.as_mut_ptr() as *mut c_void,
                buf.len(),
                &mut read,
            )
        };
        ok != 0 && read == buf.len()
    }

    pub fn read_bytes(&self, addr: usize, len: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.read_into(addr, &mut buf).then_some(buf)
    }

    /// 任意の Copy 型を読む (f32 / u32 / u64 など)
    pub fn read<T: Copy>(&self, addr: usize) -> Option<T> {
        let mut val = MaybeUninit::<T>::uninit();
        let buf =
            unsafe { std::slice::from_raw_parts_mut(val.as_mut_ptr() as *mut u8, size_of::<T>()) };
        if self.read_into(addr, buf) {
            Some(unsafe { val.assume_init() })
        } else {
            None
        }
    }

    pub fn write_bytes(&self, addr: usize, data: &[u8]) -> bool {
        let mut written = 0usize;
        let ok = unsafe {
            WriteProcessMemory(
                self.handle,
                addr as *const c_void,
                data.as_ptr() as *const c_void,
                data.len(),
                &mut written,
            )
        };
        ok != 0 && written == data.len()
    }

    pub fn write<T: Copy>(&self, addr: usize, val: T) -> bool {
        let buf =
            unsafe { std::slice::from_raw_parts(&val as *const T as *const u8, size_of::<T>()) };
        self.write_bytes(addr, buf)
    }

    /// コード領域への書き込み (保護属性を一時的に外す)
    pub fn write_code(&self, addr: usize, data: &[u8]) -> bool {
        unsafe {
            let mut old = 0u32;
            if VirtualProtectEx(
                self.handle,
                addr as *const c_void,
                data.len(),
                PAGE_EXECUTE_READWRITE,
                &mut old,
            ) == 0
            {
                return false;
            }
            let ok = self.write_bytes(addr, data);
            let mut tmp = 0u32;
            VirtualProtectEx(self.handle, addr as *const c_void, data.len(), old, &mut tmp);
            FlushInstructionCache(self.handle, addr as *const c_void, data.len());
            ok
        }
    }

    /// CE 形式のポインタチェーンを解決する。
    /// `[[base+rva]+o1]+o2` は `resolve(rva, &[o1, o2])`。
    pub fn resolve(&self, rva: usize, offsets: &[usize]) -> Option<usize> {
        let mut addr = self.base + rva;
        for &off in offsets {
            let ptr: u64 = self.read(addr)?;
            if ptr == 0 {
                return None;
            }
            addr = ptr as usize + off;
        }
        Some(addr)
    }

    /// メインモジュール全体を読み出す (AOB スキャン用)。読めないページは 0 埋め。
    pub fn dump_module(&self) -> Vec<u8> {
        const CHUNK: usize = 0x10000;
        const PAGE: usize = 0x1000;
        let mut out = vec![0u8; self.size];
        let mut off = 0;
        while off < self.size {
            let len = CHUNK.min(self.size - off);
            if !self.read_into(self.base + off, &mut out[off..off + len]) {
                // チャンクごと読めなければページ単位で読み直す
                let mut p = off;
                while p < off + len {
                    let plen = PAGE.min(off + len - p);
                    if !self.read_into(self.base + p, &mut out[p..p + plen]) {
                        out[p..p + plen].fill(0);
                    }
                    p += plen;
                }
            }
            off += len;
        }
        out
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

/// "41 BD ?? 00" 形式のパターンを解析する
pub fn parse_pattern(s: &str) -> Vec<Option<u8>> {
    s.split_whitespace()
        .map(|t| {
            if t.starts_with('?') {
                None
            } else {
                Some(u8::from_str_radix(t, 16).expect("不正な AOB パターン"))
            }
        })
        .collect()
}

/// パターンに一致する位置をすべて返す
pub fn find_pattern(hay: &[u8], pat: &[Option<u8>]) -> Vec<usize> {
    if pat.is_empty() || hay.len() < pat.len() {
        return Vec::new();
    }
    let first = pat[0];
    let mut hits = Vec::new();
    for i in 0..=hay.len() - pat.len() {
        if let Some(b) = first {
            if hay[i] != b {
                continue;
            }
        }
        if pat
            .iter()
            .zip(&hay[i..])
            .all(|(p, h)| p.is_none_or(|b| b == *h))
        {
            hits.push(i);
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_with_wildcards() {
        let pat = parse_pattern("41 ?? 01");
        let hay = [0x00, 0x41, 0xFF, 0x01, 0x41, 0x00, 0x02, 0x41, 0x12, 0x01];
        assert_eq!(find_pattern(&hay, &pat), vec![1, 7]);
    }

    static mut TARGET: [u64; 2] = [0, 0x1234_5678_9ABC_DEF0];

    /// 自分自身のプロセスに接続して読み書き・ポインタ解決・AOB を確認する
    #[test]
    fn attach_to_self() {
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_name().unwrap().to_str().unwrap();
        let p = Process::attach(name).expect("自プロセスに接続できない");
        assert_eq!(p.pid, std::process::id());
        assert!(p.is_alive());

        let addr = std::ptr::addr_of_mut!(TARGET) as usize;
        // TARGET[0] に TARGET 自身のアドレスを入れてポインタチェーンを作る
        assert!(p.write::<u64>(addr, addr as u64));
        assert_eq!(p.read::<u64>(addr + 8), Some(0x1234_5678_9ABC_DEF0));
        let rva = addr - p.base;
        assert_eq!(p.resolve(rva, &[8]), Some(addr + 8));

        // モジュールのダンプに TARGET[1] の値が含まれる
        let image = p.dump_module();
        assert_eq!(image.len(), p.size);
        let pat = parse_pattern("F0 DE BC 9A 78 56 34 12");
        assert!(find_pattern(&image, &pat).contains(&(rva + 8)));
    }

    #[test]
    fn pattern_longer_than_haystack() {
        assert!(find_pattern(&[1, 2], &parse_pattern("01 02 03")).is_empty());
    }
}
