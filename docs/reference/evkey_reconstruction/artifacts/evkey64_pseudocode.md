# EVKey64.exe — static pseudocode export

- SHA256: `5490731bb563cd22d36f3816c4eef9ed9c3b487cdc6023330002a59858e2f5ad`
- Provider: radare2 `aaa` + `pdc` (REA/Ghidra unavailable on this host)
- Static only; the executable was not run.
- This is decompiler pseudocode, not original source. Types, names, switch recovery, and some function boundaries are imperfect. Evidence addresses remain authoritative over inferred labels.

## pe_entry

```c
===== FUNCTION pe_entry 0x140048140 =====
[{"addr":5369004352,"name":"entry0","size":390,"is-pure":"false","realsz":363,"noreturn":false,"recursive":false,"stackframe":96,"calltype":"ms","cost":157,"cc":12,"bits":64,"type":"fcn","nbbs":22,"tracecov":0,"is-lineal":false,"ninstrs":96,"edges":30,"ebbs":2,"signature":"entry0 ();","minaddr":5369003980,"maxaddr":5369004370,"maxbbins":12,"midbbins":4.364,"ratbbins":2.750,"indegree":0,"outdegree":22,"nlocals":2,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int entry0 (int rcx, int rdx) {
    loc_0x140047fcc:
        // CODE XREF from entry0 @ 0x14004814d(x)
        qword [var_8h] = rbx
        push (rdi)
        rsp -= 0x30
        ecx = 1      // int64_t arg1
        fcn.140047564 () // fcn.140047564(0x0)
        v = al & al
        if (!v) goto loc_0x140048118 // likely
        goto loc_0x140047fe8;
    loc_0x140048118:
        // CODE XREF from entry0 @ 0x140047fe2(x)
        ecx = 7      // int64_t arg1
        fcn.1400482e4 () // fcn.1400482e4(0x0)
        
    loc_0x140048123:
        // CODE XREF from entry0 @ 0x140048000(x)
        ecx = 7      // int64_t arg1
        fcn.1400482e4 () // fcn.1400482e4(0x0)
        goto loc_0x140048006;
        return rax;
    loc_0x140048006:
        v = ecx & ecx
        if (v) goto loc_0x140048054 // unlikely
        goto loc_0x14004800a;
    loc_0x140048054:
        // CODE XREF from entry0 @ 0x140048008(x)
        dil = 1
        byte [var_20h] = dil
        
    loc_0x14004805c:
        // CODE XREF from entry0 @ 0x140048052(x)
        cl = bl
        fcn.140047724 ()
        fcn.1400486a4 ()
        rbx = rax
        v = qword [rax] - 0
        if (!v) goto loc_0x14004808f // unlikely
        goto loc_0x140048071;
        goto loc_0x14004802b;
        return rax;
    loc_0x14004802b:
        eax = 0xff
        goto loc_0x14004810d
        
    loc_0x14004810d:
        // CODE XREFS from entry0 @ 0x140048030(x), 0x1400480f2(x)
        rbx = qword [var_8h]
        rsp += 0x30
        rdi = pop ()
        return
        return rax;
    loc_0x14004800a: // orphan
         dword [0x1400a5b28] = 1  // [0x1400a5b28:4]=1
         rdx = rip + 0x22db5      // 0x14006add0 // int64_t arg2
         rcx = rip + 0x22d76      // 0x14006ad98 // uint32_t arg1
         fcn.14005b074 ()         // fcn.14005b074(0x0, 0x0, 0x14006add0, 0x14006ad98)
         v = eax & eax
         if (!v) 
    loc_0x140048035: // orphan
         // CODE XREF from entry0 @ 0x140048029(x)
         rdx = rip + 0x22d54      // 0x14006ad90 // int64_t arg2
         rcx = rip + 0x22bb5      // 0x14006abf8 // uint32_t arg1
         fcn.14005affc ()         // fcn.14005affc(0x0, 0x0)
         dword [0x1400a5b28] = 2  // [0x1400a5b28:4]=1
         
    loc_0x14004805c: // orphan
         // CODE XREF from entry0 @ 0x140048052(x)
         cl = bl
         fcn.140047724 ()
         fcn.1400486a4 ()
         rbx = rax
         v = qword [rax] - 0
         if (!v) 
         goto loc_0x140048071
    loc_0x140048071: // orphan
         rcx = rax                // int64_t arg1
         fcn.140047688 ()         // fcn.140047688(0x0)
         v = al & al
         if (!v) 
    loc_0x14004807d: // orphan
         r8d = 0
         edx = r8 + 2             // reloc.OLEAUT32.dll_SysReAllocString
         ecx = 0
         rax = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         qword [case.0x140047aea.0] () // [0x14006abf0:8]=0x140067250 case.0x140047aea.0 // 0x14006abf0(0x0, 0x0, 0x2, 0x0)

    loc_0x14004808f: // orphan
         // CODE XREFS from entry0 @ 0x14004806f(x), 0x14004807b(x)
         fcn.1400486ac ()
         rbx = rax
         v = qword [rax] - 0
         if (!v) 
         goto loc_0x14004809d
    loc_0x14004809d: // orphan
         rcx = rax                // int64_t arg1
         fcn.140047688 ()         // fcn.140047688(0x0)
         v = al & al
         if (!v) 
    loc_0x1400480a9: // orphan
         rcx = qword [rbx]        // int64_t arg1 // reloc.OLEAUT32.dll_SysStringLen
         fcn.14005a008 ()         // fcn.14005a008(0x0)

    loc_0x1400480b1: // orphan
         // CODE XREFS from entry0 @ 0x14004809b(x), 0x1400480a7(x)
         fcn.140048430 ()
         ebx = ax
         fcn.14005af8c ()
         r8 = rax
         r9d = ebx
         edx = 0                  // int64_t arg2
         rcx = rip - 0x480cd      // 0x140000000 // int64_t arg1
         fcn.140034f90 ()         // fcn.140034f90(0x0, 0x0)
         ebx = eax
         fcn.140048478 ()
         v = al & al
         if (!v) 
         goto loc_0x1400480dd
    loc_0x1400480dd: // orphan
         v = dil & dil
         if (v) 
         goto loc_0x1400480e2
    loc_0x1400480e2: // orphan
         fcn.140059fec ()

    loc_0x1400480e7: // orphan
         // CODE XREF from entry0 @ 0x1400480e0(x)
         edx = 0
         cl = 1
         fcn.140047748 ()
         eax = ebx
         
    loc_0x14004810d: // orphan
         // CODE XREFS from entry0 @ 0x140048030(x), 0x1400480f2(x)
         rbx = qword [var_8h]
         rsp += 0x30
         rdi = pop ()
         return

        return rax;
    loc_0x140048123: // orphan
         // CODE XREF from entry0 @ 0x140048000(x)
         ecx = 7                  // int64_t arg1
         fcn.1400482e4 ()         // fcn.1400482e4(0x0)

    loc_0x14004812d: // orphan
         // CODE XREF from entry0 @ 0x1400480db(x)
         ecx = ebx
         fcn.14005a04c ()
          // CODE XREF from fcn.140047fb0 @ +0x14d(x)
         ecx = ebx
         fcn.140059ffc ()
         int3

        return rax;
    loc_0x140048140: // orphan
         rsp -= 0x28
         fcn.1400485b4 ()
         rsp += 0x28
         
}

```

## config_registry_read

```c
===== FUNCTION config_registry_read 0x14000b310 =====
[{"addr":5368754960,"name":"fcn.14000b310","size":1444,"is-pure":"false","realsz":1444,"noreturn":false,"recursive":false,"stackframe":544,"calltype":"ms","cost":529,"cc":29,"bits":64,"type":"fcn","nbbs":46,"tracecov":0,"is-lineal":true,"ninstrs":369,"edges":73,"ebbs":1,"signature":"fcn.14000b310 ();","minaddr":5368754960,"maxaddr":5368756404,"maxbbins":48,"midbbins":8.022,"ratbbins":5.984,"indegree":20,"outdegree":49,"nlocals":13,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.14000b310 (int rcx, int rdx) {
    loc_0x14000b310:
        // XREFS: CALL 0x14000e922  CALL 0x14000eab2  CALL 0x14000eb47
        // XREFS: CALL 0x14000ecb2  CALL 0x14000ed22  CALL 0x14000eda9
        // XREFS: CALL 0x14000ef8d  CALL 0x14000f2b2  CALL 0x14000f322
        // XREFS: CALL 0x14000f3fa  CALL 0x140011026  CALL 0x1400110db
        // XREFS: CALL 0x14001112b  CALL 0x140011473  CALL 0x1400115d2
        // XREFS: CALL 0x140011652  CALL 0x1400116f1  CALL 0x140011891
        // XREFS: CALL 0x140011a22  CALL 0x140011a92
        rax = rsp
        push (rbp)
        push (r12)
        push (r13)
        push (r14)
        push (r15)
        rbp = rax - 0xf8
        rsp -= 0x1d0
        qword [rbp - 8] = 0xfffffffffffffffe
        qword [rax + 8] = rbx
        qword [rax + 0x10] = rsi
        qword [rax + 0x18] = rdi
        rax = qword [0x14009a9e8] // [0x14009a9e8:8]=0x2b992ddfa200
        rax ^= rsp
        qword [rbp + 0xc0] = rax
        rdx = rip + 0x911fa // 0x14009c550 // uint32_t arg2
        rcx = rbp - 0x60 // int64_t arg1
        fcn.140012fd0 () // fcn.140012fd0(0x0, 0x0)
        rcx = rbp - 0x60 // int64_t arg1
        fcn.140013490 () // fcn.140013490(0x0)
        v = al & al
        if (v) goto loc_0x14000b869 // likely
        goto loc_0x14000b371;
    loc_0x14000b869:
        // CODE XREF from fcn.14000b310 @ 0x14000b36b(x)
        rcx = rbp - 0x40
        fcn.14000a9a0 () // fcn.14000a9a0(0x0)
        rcx = rbp - 0x50
        fcn.14000aa30 () // fcn.14000aa30(0x0)
        rcx = qword [rbp - 0x60]
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        rcx = qword [rbp + 0xc0]
        rcx ^= rsp
        fcn.140047380 () // fcn.140047380(0x0)
        r11 = rsp + 0x1d0
        rbx = qword [r11 + 0x30]
        rsi = qword [r11 + 0x38]
        rdi = qword [r11 + 0x40]
        rsp = r11
        r15 = pop ()
        r14 = pop ()
        r13 = pop ()
        r12 = pop ()
        rbp = pop ()
        return
        goto loc_0x14000b3d1;
        return rax;
    loc_0x14000b3d1:
        dword [var_sp_40h] = 4
        rax = var_sp_40h
        qword [lpcbData] = rax // LPDWORD lpcbData
        rax = rbp - 0x10
        qword [phkResult] = rax // LPBYTE lpData
        r9 = lpType  // LPDWORD lpType
        r8d = 0      // LPDWORD lpReserved
        rdx = rip + str.SGui // 0x14006cc40 // u"SGui" // LPCWSTR lpValueName
        rcx = qword [hKey] // HKEY hKey
        qword [sym.imp.ADVAPI32.dll_RegQueryValueExW] () // [0x14006a010:8]=0x91666 reloc.ADVAPI32.dll_RegQueryValueExW // "f\x16\t" // (pstr 0xffffffff) "\n" reloc.ADVAPI32.dll_RegQueryValueExW // LSTATUS RegQueryValueExW(-1, "S", 0, 0x0000000000000000, -1, -1)
        v = eax & eax
        if (v) goto loc_0x14000b4b9 // likely
        goto loc_0x14000b40e;
    loc_0x14000b4b9:
        // CODE XREF from fcn.14000b310 @ 0x14000b408(x)
        rcx = qword [hKey] // HKEY hKey
        qword [sym.imp.ADVAPI32.dll_RegCloseKey] () // [0x14006a000:8]=0x91648 reloc.ADVAPI32.dll_RegCloseKey // "H\x16\t" // (pstr 0xffffffff) "\n" reloc.ADVAPI32.dll_RegCloseKey // LSTATUS RegCloseKey(-1)
        
    loc_0x14000b4c4:
        // CODE XREF from fcn.14000b310 @ 0x14000b3cb(x)
        r12 = rip + 0x5faed // 0x14006afb8
        rdx = r12    // uint32_t arg2
        rax = qword [0x14009c540] // [0x14009c540:8]=0
        v = rax & rax
        if (!zf) rdx = rax
        r8d = 0x80   // int64_t arg3
        rcx = rbp - 0x30 // int64_t arg1
        fcn.140004e50 () // fcn.140004e50(0x0, 0x0, 0x14006afb8)
        r13 = qword [rbp - 0x78]
        rax = qword [rbp - 0x70]
        r15 = rax*8
        r15 += r13
        rbx = r13
        v = r13 - r15
        if (!v) goto loc_0x14000b574 // likely
        goto loc_0x14000b507;
        goto loc_0x14000b430;
        return rax;
    loc_0x14000b430:
        // CODE XREF from fcn.14000b310 @ 0x14000b493(x)
        v = r14d & r14d
        if (v) goto loc_0x14000b473 // unlikely
        goto loc_0x14000b435;
    loc_0x14000b473:
        // CODE XREF from fcn.14000b310 @ 0x14000b433(x)
        r14d = 1
        
    loc_0x14000b479:
        // CODE XREF from fcn.14000b310 @ 0x14000b471(x)
        rcx = qword [rsi] // reloc.OLEAUT32.dll_SysStringLen
        rbx = qword [rcx] // (pstr 0xffffffff) "\n"
        qword [rbx + 0x18] () // 0x18(0x0, 0x0, 0x0, 0x6)
        rdx = rax + 1
        rcx = qword [rsi] // reloc.OLEAUT32.dll_SysStringLen
        qword [rbx + 0x30] () // 0x30(0x0, 0x0, 0x1, 0x6)
        rsi += 8     // reloc.OLEAUT32.dll_VariantClear
        v = rsi - r15
        if (v) goto loc_0x14000b430 // likely
        goto loc_0x14000b495;
        goto loc_0x14000b473;
        return rax;
        return rax;
    loc_0x14000b495:
        // CODE XREF from fcn.14000b310 @ 0x14000b42e(x)
        rcx = qword [hKey] // HKEY hKey
        qword [sym.imp.ADVAPI32.dll_RegCloseKey] () // [0x14006a000:8]=0x91648 reloc.ADVAPI32.dll_RegCloseKey // "H\x16\t" // (pstr 0xffffffff) "\n" reloc.ADVAPI32.dll_RegCloseKey // LSTATUS RegCloseKey(-1)
        rdx = rip + str.SoftwareMyQEVKey // 0x14006cc50 // u"Software\MyQ\EVKey" // LPCWSTR pszSubKey
        rcx = 0xffffffff80000001 // HKEY hkey
        qword [sym.imp.SHLWAPI.dll_SHDeleteKeyW] () // [0x14006a628:8]=0x91754 reloc.SHLWAPI.dll_SHDeleteKeyW // "T\x17\t" // (pstr 0xffffffff) "\n" reloc.SHLWAPI.dll_SHDeleteKeyW // LSTATUS SHDeleteKeyW(-1, "S")
        goto loc_0x14000b57d
        
    loc_0x14000b57d:
        // CODE XREF from fcn.14000b310 @ 0x14000b4b4(x)
        rdx = qword [0x1400946d8] // [0x1400946d8:8]=0x14006c768 str.setting // uint32_t arg2 // u"setting" str.setting
        rcx = rbp - 0x50 // int64_t arg1
        fcn.14000bbb0 () // fcn.14000bbb0(0x0, 0x0)
        r14 = rdi
        rsi = r15
        rsi -= r13
        rsi += 7
        rsi >>>= 3
        v = r13 - r15
        cmova rsi rdi
        rbx = r13
        v = rsi & rsi
        if (!v) goto loc_0x14000b5e4 // likely
        goto loc_0x14000b5ad;
        return rax;
    loc_0x14000b5ad:
        r13 = rax
        
    loc_0x14000b5b0:
        // CODE XREF from fcn.14000b310 @ 0x14000b5de(x)
        rcx = qword [rbx] // reloc.OLEAUT32.dll_SysStringLen
        rax = qword [rcx] // (pstr 0xffffffff) "\n"
        qword [rax + 0x18] () // 0x18(0x0, 0x0, 0x0, 0x6)
        r8 = rax     // int64_t arg3 // (pstr 0xffffffff) "\n"
        rdx = qword [rbx] // reloc.OLEAUT32.dll_SysStringLen
        rdx += 0x18  // int64_t arg2
        rcx = r13    // int64_t arg1
        fcn.140012ce0 () // fcn.140012ce0(0x0, 0x0, 0x1e)
        rcx = qword [rbx] // reloc.OLEAUT32.dll_SysStringLen
        rax = qword [rcx] // (pstr 0xffffffff) "\n"
        qword [rax + 0x10] () // 0x10(0x0, 0x0, 0x1e, 0x6)
        rbx = rbx + 8 // reloc.OLEAUT32.dll_VariantClear
        r14++
        v = r14 - rsi
        if (v) goto loc_0x14000b5b0 // likely
        goto loc_0x14000b5e0;
        return rax;
    loc_0x14000b5e0:
        r13 = qword [rbp - 0x78]
        
    loc_0x14000b5e4:
        // CODE XREF from fcn.14000b310 @ 0x14000b5ab(x)
        qword [hKey] = rdi
        qword [var_38h] = rdi
        rcx = hKey   // int64_t arg1
        fcn.1400111b0 () // fcn.1400111b0(0x0)
        v = al & al
        if (v) goto loc_0x14000b60d // unlikely
        goto loc_0x14000b5fc;
        return rax;
    loc_0x14000b5fc:
        rdx = hKey   // signed int64_t arg2
        rcx = qword [0x1400946e0] // [0x1400946e0:8]=0x14006c778 str.blacklist // int64_t arg1 // u"blacklist" str.blacklist
        fcn.14000fbf0 () // fcn.14000fbf0(0x0, 0x0)
        
    loc_0x14000b60d:
        // CODE XREF from fcn.14000b310 @ 0x14000b5fa(x)
        r8 = hKey    // int64_t arg3
        rdx = qword [0x1400946e0] // [0x1400946e0:8]=0x14006c778 str.blacklist // uint32_t arg2 // u"blacklist" str.blacklist
        rcx = rbp - 0x50 // int64_t arg1
        fcn.140012660 () // fcn.140012660(0x0, 0x0, 0x14006c778)
        qword [var_sp_78h] = rdi
        qword [rbp - 0x80] = rdi
        qword [var_sp_68h] = rdi
        qword [var_sp_70h] = rdi
        rdx = var_sp_78h // signed int64_t arg2
        rcx = qword [0x1400946e8] // [0x1400946e8:8]=0x14006c790 // int64_t arg1 // u"on_vn"
        fcn.14000fbf0 () // fcn.14000fbf0(0x0, 0x0)
        rdx = var_sp_68h // signed int64_t arg2
        rcx = qword [0x1400946f0] // [0x1400946f0:8]=0x14006c7a0 // int64_t arg1 // u"off_vn"
        fcn.14000fbf0 () // fcn.14000fbf0(0x0, 0x0)
        r8 = var_sp_78h // int64_t arg3
        rdx = qword [0x1400946e8] // [0x1400946e8:8]=0x14006c790 // uint32_t arg2 // u"on_vn"
        rcx = rbp - 0x50 // int64_t arg1
        fcn.140012660 () // fcn.140012660(0x0, 0x0, 0x14006c790)
        r8 = var_sp_68h // int64_t arg3
        rdx = qword [0x1400946f0] // [0x1400946f0:8]=0x14006c7a0 // uint32_t arg2 // u"off_vn"
        rcx = rbp - 0x50 // int64_t arg1
        fcn.140012660 () // fcn.140012660(0x0, 0x0, 0x14006c7a0)
        qword [rbp - 0x30] = rdi
        qword [rbp - 0x28] = rdi
        qword [rbp - 0x20] = rdi
        rcx = rbp - 0x30 // int64_t arg1
        fcn.14000fdb0 () // fcn.14000fdb0(0x0)
        rdx = rbp - 0x30 // int64_t arg2
        rcx = rbp - 0x50 // int64_t arg1
        fcn.140012840 () // fcn.140012840(0x0, 0x0)
        qword [var_58h] = rdi
        qword [var_sp_60h] = rdi
        rcx = var_58h // signed int64_t arg1
        fcn.14000f4d0 () // fcn.14000f4d0(0x0)
        rdx = var_58h // int64_t arg2
        rcx = rbp - 0x50 // int64_t arg1
        fcn.140011af0 () // fcn.140011af0(0x0, 0x0)
        qword [var_48h] = rdi
        qword [var_sp_50h] = rdi
        rcx = var_48h // signed int64_t arg1 // rsp
        fcn.14000f800 () // fcn.14000f800(0x0)
        rdx = var_48h // int64_t arg2
        rcx = rbp - 0x50 // int64_t arg1 // "\u0676"
        fcn.140011d30 () // fcn.140011d30(0x0, 0x0)
        rcx = rbp - 0x60 // int64_t arg1 // (pstr 0xffffffff) "\n"
        fcn.140013e20 () // fcn.140013e20(0x0)
        v = al & al
        if (!v) goto loc_0x14000b728 // likely
        goto loc_0x14000b6f4;
        return rax;
    loc_0x14000b6f4:
        rcx = r12
        rax = qword [0x14009c540] // [0x14009c540:8]=0
        v = rax & rax
        if (!zf) rcx = rax
        edx = 0
        fcn.140053f44 ()
        v = eax & eax
        if (v) goto loc_0x14000b728 // unlikely
        goto loc_0x14000b710;
    loc_0x14000b728:
        // CODE XREFS from fcn.14000b310 @ 0x14000b6f2(x), 0x14000b70e(x)
        r14 = qword [var_48h]
        eax = dword [var_sp_50h]
        v = eax & eax
        if (v <= 0) goto loc_0x14000b752 // unlikely
        goto loc_0x14000b735;
    loc_0x14000b752:
        // CODE XREF from fcn.14000b310 @ 0x14000b733(x)
        rcx = r14
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        r14 = qword [var_58h]
        eax = dword [var_sp_60h]
        v = eax & eax
        if (v <= 0) goto loc_0x14000b782 // likely
        goto loc_0x14000b768;
    loc_0x14000b782:
        // CODE XREF from fcn.14000b310 @ 0x14000b766(x)
        rcx = r14
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        rcx = qword [rbp - 0x30]
        fcn.1400474e4 ()
        r14 = qword [var_sp_68h]
        eax = dword [var_sp_70h]
        v = eax & eax
        if (v <= 0) goto loc_0x14000b7b9 // likely
        goto loc_0x14000b7a2;
    loc_0x14000b7b9:
        // CODE XREF from fcn.14000b310 @ 0x14000b7a0(x)
        rcx = r14
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        r14 = qword [var_sp_78h]
        eax = dword [rbp - 0x80]
        v = eax & eax
        if (v <= 0) goto loc_0x14000b7e5 // likely
        goto loc_0x14000b7ce;
    loc_0x14000b7e5:
        // CODE XREF from fcn.14000b310 @ 0x14000b7cc(x)
        rcx = r14
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        r14 = qword [hKey]
        eax = dword [var_38h]
        v = eax & eax
        if (v <= 0) goto loc_0x14000b812 // likely
        goto loc_0x14000b7fb;
    loc_0x14000b812:
        // CODE XREF from fcn.14000b310 @ 0x14000b7f9(x)
        rcx = r14
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        rsi = r15
        rsi -= r13
        rsi += 7
        rsi >>>= 3
        v = r13 - r15
        cmova rsi rdi
        rbx = r13
        v = rsi & rsi
        if (!v) goto loc_0x14000b856 // likely
        goto loc_0x14000b838;
    loc_0x14000b856:
        // CODE XREF from fcn.14000b310 @ 0x14000b836(x)
        rcx = r13
        fcn.1400474e4 ()
        rcx = rbp    // rsp
        fcn.14000aae0 () // fcn.14000aae0(0x0)
    loc_0x14000b40e: // orphan
         r13 = qword [rbp - 0x78]
         rax = qword [rbp - 0x70]
         r15 = rax*8
         r15 += r13
         rsi = r13
         r12 = rip + 0x5fb8d      // 0x14006afb8
         v = r13 - r15
         if (!v) 
    loc_0x14000b435: // orphan
         rcx = qword [rsi]        // reloc.OLEAUT32.dll_SysStringLen
         rax = qword [rcx + 0x18]
         rdx = r12
         v = rax & rax
         if (!zf) rdx = rax       // LPCWSTR lpValueName
         rax = var_sp_40h
         qword [lpcbData] = rax   // LPDWORD lpcbData
         rax = qword [rcx + 8]
         qword [phkResult] = rax  // LPBYTE lpData
         r9 = lpType              // LPDWORD lpType
         r8d = 0                  // LPDWORD lpReserved
         rcx = qword [hKey]       // HKEY hKey
         qword [sym.imp.ADVAPI32.dll_RegQueryValueExW] () // [0x14006a010:8]=0x91666 reloc.ADVAPI32.dll_RegQueryValueExW // "f\x16\t" // (pstr 0xffffffff) "\n" reloc.ADVAPI32.dll_RegQueryValueExW // LSTATUS RegQueryValueExW(0, 0, 0, 0x0000000000000000, -1, -1)
         v = eax & eax
         r14d = edi
         if (!v) 
    loc_0x14000b479: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b471(x)
         rcx = qword [rsi]        // reloc.OLEAUT32.dll_SysStringLen
         rbx = qword [rcx]        // (pstr 0xffffffff) "\n"
         qword [rbx + 0x18] ()    // 0x18(0x0, 0x0, 0x0, 0x6)
         rdx = rax + 1
         rcx = qword [rsi]        // reloc.OLEAUT32.dll_SysStringLen
         qword [rbx + 0x30] ()    // 0x30(0x0, 0x0, 0x1, 0x6)
         rsi += 8                 // reloc.OLEAUT32.dll_VariantClear
         v = rsi - r15
         if (v) 
    loc_0x14000b4c4: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b3cb(x)
         r12 = rip + 0x5faed      // 0x14006afb8
         rdx = r12                // uint32_t arg2
         rax = qword [0x14009c540] // [0x14009c540:8]=0
         v = rax & rax
         if (!zf) rdx = rax
         r8d = 0x80               // int64_t arg3
         rcx = rbp - 0x30         // int64_t arg1
         fcn.140004e50 ()         // fcn.140004e50(0x0, 0x0, 0x14006afb8)
         r13 = qword [rbp - 0x78]
         rax = qword [rbp - 0x70]
         r15 = rax*8
         r15 += r13
         rbx = r13
         v = r13 - r15
         if (!v) 
         goto loc_0x14000b507
    loc_0x14000b507: // orphan
         

    loc_0x14000b510: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b572(x)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         rax = qword [rcx]        // (pstr 0xffffffff) "\n"
         rdx = 0xffffffffffffffff
         qword [rax + 0x30] ()    // 0x30(0x0, 0x0, -1, 0x6)
         rax = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         rcx = qword [rax + 0x18]
         r8 = r12
         v = rcx & rcx
         if (!zf) r8 = rcx        // int64_t arg3
         r9 = 0xffffffffffffffff  // int64_t arg4
         rdx = qword [0x1400946d8] // [0x1400946d8:8]=0x14006c768 str.setting // int64_t arg2 // u"setting" str.setting
         rcx = rbp - 0x30         // int64_t arg1
         fcn.140004f90 ()         // fcn.140004f90(0x0, 0x0, 0x14006c768, 0x177fd0)
         rsi = rax                // reloc.OLEAUT32.dll_SysStringLen
         v = rax - 0xffffffffffffffff
         if (!v) 
         goto loc_0x14000b551
    loc_0x14000b551: // orphan
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         r8 = qword [rcx]         // (pstr 0xffffffff) "\n"
         rdx = rax
         qword [r8 + 0x20] ()     // 0x20(0x0, 0x0, 0x0, 0x6)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         r8 = qword [rcx]         // (pstr 0xffffffff) "\n"
         rdx = rsi
         qword [r8 + 0x30] ()     // 0x30(0x0, 0x0, 0x0, 0x6)

    loc_0x14000b56b: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b54f(x)
         rbx += 8                 // reloc.OLEAUT32.dll_VariantClear
         v = rbx - r15
         if (v) 
         goto loc_0x14000b574
    loc_0x14000b574: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b505(x)
         rcx = rbp - 0x30         // int64_t arg1
         fcn.140004f20 ()         // fcn.140004f20(0x0)

    loc_0x14000b57d: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b4b4(x)
         rdx = qword [0x1400946d8] // [0x1400946d8:8]=0x14006c768 str.setting // uint32_t arg2 // u"setting" str.setting
         rcx = rbp - 0x50         // int64_t arg1 // "\u0676"
         fcn.14000bbb0 ()         // fcn.14000bbb0(0x0, 0x0)
         r14 = rdi
         rsi = r15
         rsi -= r13
         rsi += 7
         rsi >>>= 3
         v = r13 - r15
         cmova rsi rdi
         rbx = r13
         v = rsi & rsi
         if (!v) 
    loc_0x14000b5b0: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b5de(x)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         rax = qword [rcx]        // (pstr 0xffffffff) "\n"
         qword [rax + 0x18] ()    // 0x18(0x0, 0x0, 0x0, 0x6)
         r8 = rax                 // int64_t arg3 // (pstr 0xffffffff) "\n"
         rdx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         rdx += 0x18              // int64_t arg2
         rcx = r13                // int64_t arg1
         fcn.140012ce0 ()         // fcn.140012ce0(0x0, 0x0, 0x1e)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         rax = qword [rcx]        // (pstr 0xffffffff) "\n"
         qword [rax + 0x10] ()    // 0x10(0x0, 0x0, 0x1e, 0x6)
         rbx = rbx + 8            // reloc.OLEAUT32.dll_VariantClear
         r14++
         v = r14 - rsi
         if (v) 
    loc_0x14000b5e4: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b5ab(x)
         qword [hKey] = rdi
         qword [var_38h] = rdi
         rcx = hKey               // int64_t arg1
         fcn.1400111b0 ()         // fcn.1400111b0(0x0)
         v = al & al
         if (v) 
    loc_0x14000b60d: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b5fa(x)
         r8 = hKey                // int64_t arg3
         rdx = qword [0x1400946e0] // [0x1400946e0:8]=0x14006c778 str.blacklist // uint32_t arg2 // u"blacklist" str.blacklist
         rcx = rbp - 0x50         // int64_t arg1 // "\u0676"
         fcn.140012660 ()         // fcn.140012660(0x0, 0x0, 0x14006c778)
         qword [var_sp_78h] = rdi
         qword [rbp - 0x80] = rdi
         qword [var_sp_68h] = rdi
         qword [var_sp_70h] = rdi
         rdx = var_sp_78h         // signed int64_t arg2
         rcx = qword [0x1400946e8] // [0x1400946e8:8]=0x14006c790 // int64_t arg1 // u"on_vn"
         fcn.14000fbf0 ()         // fcn.14000fbf0(0x0, 0x0)
         rdx = var_sp_68h         // signed int64_t arg2
         rcx = qword [0x1400946f0] // [0x1400946f0:8]=0x14006c7a0 // int64_t arg1 // u"off_vn"
         fcn.14000fbf0 ()         // fcn.14000fbf0(0x0, 0x0)
         r8 = var_sp_78h          // int64_t arg3
         rdx = qword [0x1400946e8] // [0x1400946e8:8]=0x14006c790 // uint32_t arg2 // u"on_vn"
         rcx = rbp - 0x50         // int64_t arg1 // "\u0676"
         fcn.140012660 ()         // fcn.140012660(0x0, 0x0, 0x14006c790)
         r8 = var_sp_68h          // int64_t arg3
         rdx = qword [0x1400946f0] // [0x1400946f0:8]=0x14006c7a0 // uint32_t arg2 // u"off_vn"
         rcx = rbp - 0x50         // int64_t arg1 // "\u0676"
         fcn.140012660 ()         // fcn.140012660(0x0, 0x0, 0x14006c7a0)
         qword [rbp - 0x30] = rdi
         qword [rbp - 0x28] = rdi
         qword [rbp - 0x20] = rdi
         rcx = rbp - 0x30         // int64_t arg1
         fcn.14000fdb0 ()         // fcn.14000fdb0(0x0)
         rdx = rbp - 0x30         // int64_t arg2
         rcx = rbp - 0x50         // int64_t arg1 // "\u0676"
         fcn.140012840 ()         // fcn.140012840(0x0, 0x0)
         qword [var_58h] = rdi
         qword [var_sp_60h] = rdi
         rcx = var_58h            // signed int64_t arg1
         fcn.14000f4d0 ()         // fcn.14000f4d0(0x0)
         rdx = var_58h            // int64_t arg2
         rcx = rbp - 0x50         // int64_t arg1 // "\u0676"
         fcn.140011af0 ()         // fcn.140011af0(0x0, 0x0)
         qword [var_48h] = rdi
         qword [var_sp_50h] = rdi
         rcx = var_48h            // signed int64_t arg1 // rsp
         fcn.14000f800 ()         // fcn.14000f800(0x0)
         rdx = var_48h            // int64_t arg2
         rcx = rbp - 0x50         // int64_t arg1 // "\u0676"
         fcn.140011d30 ()         // fcn.140011d30(0x0, 0x0)
         rcx = rbp - 0x60         // int64_t arg1
         fcn.140013e20 ()         // fcn.140013e20(0x0)
         v = al & al
         if (!v) 
    loc_0x14000b710: // orphan
         rax = qword [0x14009c540] // [0x14009c540:8]=0
         v = rax & rax
         if (!zf) r12 = rax
         rcx = r12                // LPCWSTR lpFileName
         qword [sym.imp.KERNEL32.dll_DeleteFileW] () // [0x14006a4d8:8]=0x918be reloc.KERNEL32.dll_DeleteFileW // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_DeleteFileW // BOOL DeleteFileW(0)

    loc_0x14000b735: // orphan
         rbx = r14
         esi = eax

    loc_0x14000b740: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b750(x)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rbx = rbx + 0x20
         rsi -= 1
         if (v) 
    loc_0x14000b768: // orphan
         rbx = r14
         esi = eax

    loc_0x14000b770: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b780(x)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rbx = rbx + 0x18
         rsi -= 1
         if (v) 
    loc_0x14000b7a2: // orphan
         rbx = r14
         esi = eax

    loc_0x14000b7a7: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b7b7(x)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rbx = rbx + 0x10
         rsi -= 1
         if (v) 
    loc_0x14000b7ce: // orphan
         rbx = r14
         esi = eax

    loc_0x14000b7d3: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b7e3(x)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rbx = rbx + 0x10
         rsi -= 1
         if (v) 
    loc_0x14000b7fb: // orphan
         rbx = r14
         esi = eax

    loc_0x14000b800: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b810(x)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rbx = rbx + 0x10
         rsi -= 1
         if (v) 
    loc_0x14000b838: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b854(x)
         rcx = qword [rbx]        // reloc.OLEAUT32.dll_SysStringLen
         v = rcx & rcx
         if (!v) 
         goto loc_0x14000b840
    loc_0x14000b840: // orphan
         rax = qword [rcx]        // reloc.OLEAUT32.dll_SysStringLen
         edx = 1
         qword [rax] ()           // (pstr 0xffffffff) "\n" // 0x60000(0x0, 0x0, 0x1, 0x0)

    loc_0x14000b84a: // orphan
         // CODE XREF from fcn.14000b310 @ 0x14000b83e(x)
         rbx += 8                 // reloc.OLEAUT32.dll_VariantClear
         rdi++
         v = rdi - rsi
         if (v) 
}

```

## config_load_save

```c
===== FUNCTION config_load_save 0x14000bcc0 =====
[{"addr":5368757440,"name":"fcn.14000bcc0","size":9458,"is-pure":"false","realsz":9458,"noreturn":false,"recursive":false,"stackframe":240,"calltype":"ms","cost":2900,"cc":82,"bits":64,"type":"fcn","nbbs":200,"tracecov":0,"is-lineal":true,"ninstrs":2239,"edges":280,"ebbs":1,"signature":"fcn.14000bcc0 (int64_t arg1, int64_t arg2);","minaddr":5368757440,"maxaddr":5368766898,"maxbbins":237,"midbbins":11.195,"ratbbins":21.170,"indegree":3,"outdegree":270,"nlocals":1,"nargs":2,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.14000bcc0 (int64_t arg1, int64_t arg2) {
    loc_0x14000bcc0:
        // CALL XREF from fcn.14000b310 @ 0x14000b39c(x)
        // CALL XREF from fcn.14000eb10 @ 0x14000eb42(x)
        // CALL XREF from fcn.140011450 @ 0x140011490(x)
        rax = rsp
        push (rbp)
        push (r12)
        push (r13)
        push (r14)
        push (r15)
        rbp = rax - 0x5f
        rsp -= 0xa0
        qword [rbp - 0x21] = 0xfffffffffffffffe
        qword [rax + 8] = rbx
        qword [rax + 0x18] = rsi
        qword [rax + 0x20] = rdi
        rax = qword [0x14009a9e8] // [0x14009a9e8:8]=0x2b992ddfa200
        rax ^= rsp
        qword [rbp + 0x27] = rax
        rbx = rdx    // arg2
        r13 = rcx    // arg1
        dword [rbp - 0x41] = 4
        dword [rbp - 0x3d] = 5
        r12d = 0xa
        ecx = r12d
        fcn.14004bee0 ()
        r14 = rax
        qword [rbp - 0x49] = rax
        xmm0 = qword [0x14006c870] // [0x14006c870:8]=0x790065004b0053
        qword [rax] = xmm0
        ecx = word [0x14006c878] // [0x14006c878:2]=0
        word [rax + 8] = cx
        ecx = r12 + 0x1e
        fcn.140047b24 () // fcn.140047b24(0x0)
        rdi = rax
        qword [rbp - 0x39] = rax
        r15 = rip + vtable.KeyValueTypeBase.0 // 0x14006d370
        esi = 0
        v = rax & rax
        if (!v) goto loc_0x14000bdb7 // unlikely
        goto loc_0x14000bd58;
    loc_0x14000bdb7:
        // CODE XREF from fcn.14000bcc0 @ 0x14000bd56(x)
        rdi = rsi
        
    loc_0x14000bdba:
        // CODE XREF from fcn.14000bcc0 @ 0x14000bdb5(x)
        rdx = qword [rbx + 8] // reloc.OLEAUT32.dll_SysStringLen
        rdx++
        rcx = rbx
        fcn.14000b130 () // fcn.14000b130(0x0, 0x0)
        rcx = qword [rbx + 8] // reloc.OLEAUT32.dll_SysStringLen
        rax = qword [rbx] // reloc.OLEAUT32.dll_SysStringLen
        qword [rax + rcx*8] = rdi
        qword [rbx + 8]++
        rcx = r14
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        dword [rbp - 0x41] = 4
        dword [rbp - 0x3d] = 5
        rcx = r12
        fcn.14004bee0 ()
        r14 = rax    // reloc.OLEAUT32.dll_SysStringLen
        qword [rbp - 0x49] = rax
        xmm0 = qword [0x14006c880] // [0x14006c880:8]=0x70007000410045
        qword [rax] = xmm0
        ecx = word [0x14006c888] // [0x14006c888:2]=0
        word [rax + 8] = cx
        ecx = 0x28   // '('
        fcn.140047b24 () // fcn.140047b24(0x0)
        rdi = rax    // reloc.OLEAUT32.dll_SysStringLen
        qword [rbp - 0x39] = rax
        v = rax & rax
        if (!v) goto loc_0x14000be8d // unlikely
        goto loc_0x14000be2a;
        goto loc_0x14000bd7b;
        return rax;
    loc_0x14000bd7b:
        dword [rdi + 0x20] = 4
        dword [rdi + 0x24] = 5
        ecx = r12d
        fcn.14004bee0 ()
        qword [rdi + 0x18] = rax
        ecx = dword [rdi + 0x20]
        ecx++
        r8 = ecx
        r8 += r8
        rdx = r14
        rcx = rax
        fcn.140049380 () // fcn.140049380(0x0, 0x0, 0x0)
        
    loc_0x14000bdab:
        // CODE XREF from fcn.14000bcc0 @ 0x14000bd79(x)
        rax = rip + vtable.KeyValueType_long_long___int64__.0 // 0x14006d3b0
        qword [rdi] = rax
        goto loc_0x14000bdba
        return rax;
    loc_0x14000bdab: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000bd79(x)
         rax = rip + vtable.KeyValueType_long_long___int64__.0 // 0x14006d3b0
         qword [rdi] = rax
         
    loc_0x14000bdba: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000bdb5(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]        // vtable.KeyValueType_long_long___int64__.0
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 4
         dword [rbp - 0x3d] = 5
         rcx = r12
         fcn.14004bee0 ()
         r14 = rax                // vtable.KeyValueType_long_long___int64__.0
         qword [rbp - 0x49] = rax
         xmm0 = qword [0x14006c880] // [0x14006c880:8]=0x70007000410045
         qword [rax] = xmm0
         ecx = word [0x14006c888] // [0x14006c888:2]=0
         word [rax + 8] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax                // vtable.KeyValueType_long_long___int64__.0
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000be2a
    loc_0x14000be2a: // orphan
         qword [rax] = r15
         rcx = r13 + 0x34
         qword [rax + 8] = rcx
         rax = rip + 0x90758      // 0x14009c594
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000be51
    loc_0x14000be51: // orphan
         dword [rdi + 0x20] = 4
         dword [rdi + 0x24] = 5
         rcx = r12
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000be81: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000be4f(x)
         rax = rip + vtable.KeyValueType_unsigned_long_int_.0 // 0x14006d3f0
         qword [rdi] = rax
         
         goto loc_0x14000be8d
    loc_0x14000be8d: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000be28(x)
         rdi = rsi

    loc_0x14000be90: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000be8b(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]        // vtable.KeyValueType_unsigned_long_int_.0
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 4
         dword [rbp - 0x3d] = 5
         rcx = r12
         fcn.14004bee0 ()
         r14 = rax                // vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x49] = rax
         xmm0 = qword [0x14006c890] // [0x14006c890:8]=0x69007500470053
         qword [rax] = xmm0
         ecx = word [0x14006c898] // [0x14006c898:2]=0
         word [rax + 8] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax                // vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x39] = rax
         r12 = rip + vtable.KeyValueType_unsigned_char_.0 // 0x14006d430
         v = rax & rax
         if (!v) 
         goto loc_0x14000bf07
    loc_0x14000bf07: // orphan
         qword [rax] = r15
         rcx = r13 + 2            // reloc.OLEAUT32.dll_SysReAllocString
         qword [rax + 8] = rcx
         rax = rip + 0x90649      // 0x14009c562
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000bf2e
    loc_0x14000bf2e: // orphan
         dword [rdi + 0x20] = 4
         dword [rdi + 0x24] = 5
         ecx = 0xa
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000bf60: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000bf2c(x)
         qword [rdi] = r12
         
         goto loc_0x14000bf65
    loc_0x14000bf65: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000bf05(x)
         rdi = rsi

    loc_0x14000bf68: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000bf63(x)
         rdx = qword [rbx + 8]    // reloc.OLEAUT32.dll_SysReAllocString
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]    // reloc.OLEAUT32.dll_SysReAllocString
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 4
         dword [rbp - 0x3d] = 5
         ecx = 0xa
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.Type]  // [0x14006c8a0:8]=0x65007000790054 // u"Type"
         qword [rax] = xmm0
         ecx = word [0x14006c8a8] // [0x14006c8a8:2]=0
         word [rax + 8] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000bfda
    loc_0x14000bfda: // orphan
         qword [rax] = r15
         rcx = r13 + 3
         qword [rax + 8] = rcx
         rax = rip + 0x90577      // 0x14009c563
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c001
    loc_0x14000c001: // orphan
         dword [rdi + 0x20] = 4
         dword [rdi + 0x24] = 5
         ecx = 0xa
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c033: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000bfff(x)
         qword [rdi] = r12
         
         goto loc_0x14000c038
    loc_0x14000c038: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000bfd8(x)
         rdi = rsi

    loc_0x14000c03b: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c036(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 5
         dword [rbp - 0x3d] = 6
         ecx = 0xc
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.Codec] // [0x14006c8b0:8]=0x650064006f0043 // u"Codec"
         qword [rax] = xmm0
         ecx = dword [0x14006c8b8] // [0x14006c8b8:4]=99
         dword [rax + 8] = ecx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c0ab
    loc_0x14000c0ab: // orphan
         qword [rax] = r15
         rcx = r13 + 8            // reloc.OLEAUT32.dll_VariantClear
         qword [rax + 8] = rcx
         rax = rip + 0x904ab      // 0x14009c568
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c0d2
    loc_0x14000c0d2: // orphan
         dword [rdi + 0x20] = 5
         dword [rdi + 0x24] = 6
         ecx = 0xc
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // (pstr 0x00080000) "\"" reloc.OLEAUT32.dll_SysStringLen
         r8 = ecx                 // (pstr 0x00080000) "\"" reloc.OLEAUT32.dll_SysStringLen
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c104: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c0d0(x)
         rax = rip + vtable.KeyValueType_unsigned_long_int_.0 // 0x14006d3f0
         qword [rdi] = rax
         
         goto loc_0x14000c110
    loc_0x14000c110: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c0a9(x)
         rdi = rsi

    loc_0x14000c113: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c10e(x)
         rdx = qword [rbx + 8]    // reloc.OLEAUT32.dll_VariantClear
         rdx++                    // reloc.OLEAUT32.dll_VariantCopy
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]    // reloc.OLEAUT32.dll_VariantClear
         rax = qword [rbx]        // vtable.KeyValueType_unsigned_long_int_.0
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax                // vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.SCodec] // [0x14006c8c0:8]=0x64006f00430053 // u"SCodec"
         qword [rax] = xmm0
         ecx = dword [0x14006c8c8] // [0x14006c8c8:4]=0x630065 // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006c8cc] // [0x14006c8cc:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax                // vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c18e
    loc_0x14000c18e: // orphan
         qword [rax] = r15
         rcx = r13 + 0xc
         qword [rax + 8] = rcx
         rax = rip + 0x903cc      // 0x14009c56c
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c1b5
    loc_0x14000c1b5: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c1e7: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c1b3(x)
         rax = rip + vtable.KeyValueType_unsigned_long_int_.0 // 0x14006d3f0
         qword [rdi] = rax
         
         goto loc_0x14000c1f3
    loc_0x14000c1f3: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c18c(x)
         rdi = rsi

    loc_0x14000c1f6: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c1f1(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]        // vtable.KeyValueType_unsigned_long_int_.0
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax                // vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.SInput] // [0x14006c8d0:8]=0x70006e00490053 // u"SInput"
         qword [rax] = xmm0
         ecx = dword [0x14006c8d8] // [0x14006c8d8:4]=0x740075 // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006c8dc] // [0x14006c8dc:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax                // vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c271
    loc_0x14000c271: // orphan
         qword [rax] = r15
         rcx = r13 + 0x30
         qword [rax + 8] = rcx
         rax = rip + 0x9030d      // 0x14009c590
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c298
    loc_0x14000c298: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c2ca: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c296(x)
         qword [rdi] = r12
         
         goto loc_0x14000c2cf
    loc_0x14000c2cf: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c26f(x)
         rdi = rsi

    loc_0x14000c2d2: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c2cd(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 5
         dword [rbp - 0x3d] = 6
         ecx = 0xc
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.FMark] // [0x14006c8e0:8]=0x720061004d0046 // u"FMark"
         qword [rax] = xmm0
         ecx = dword [0x14006c8e8] // [0x14006c8e8:4]=107
         dword [rax + 8] = ecx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c342
    loc_0x14000c342: // orphan
         qword [rax] = r15
         rcx = r13 + 0x31
         qword [rax + 8] = rcx
         rax = rip + 0x9023d      // 0x14009c591
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c369
    loc_0x14000c369: // orphan
         dword [rdi + 0x20] = 5
         dword [rdi + 0x24] = 6
         ecx = 0xc
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // reloc.OLEAUT32.dll_SysStringLen
         r8 = ecx                 // reloc.OLEAUT32.dll_SysStringLen
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c39b: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c367(x)
         qword [rdi] = r12
         
         goto loc_0x14000c3a0
    loc_0x14000c3a0: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c340(x)
         rdi = rsi

    loc_0x14000c3a3: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c39e(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.SMetro] // [0x14006c8f0:8]=0x740065004d0053 // u"SMetro"
         qword [rax] = xmm0
         ecx = dword [0x14006c8f8] // [0x14006c8f8:4]=0x6f0072 // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006c8fc] // [0x14006c8fc:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c41e
    loc_0x14000c41e: // orphan
         qword [rax] = r15
         rcx = r13 + 0x32
         qword [rax + 8] = rcx
         rax = rip + 0x90162      // 0x14009c592
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c445
    loc_0x14000c445: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c477: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c443(x)
         qword [rdi] = r12
         
         goto loc_0x14000c47c
    loc_0x14000c47c: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c41c(x)
         rdi = rsi

    loc_0x14000c47f: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c47a(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 8
         dword [rbp - 0x3d] = 9
         ecx = 0x12
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.SBrowser] // [0x14006c900:16]=-1 // u"SBrowser"
         xmmword [rax] = xmm0
         ecx = word [0x14006c910] // [0x14006c910:2]=0
         word [rax + 0x10] = cx
         ecx = 0x28               // '(' // u(cstr 0x00000000) "SBrowser"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c4ef
    loc_0x14000c4ef: // orphan
         qword [rax] = r15
         rcx = r13 + 0x33
         qword [rax + 8] = rcx
         rax = rip + 0x90092      // 0x14009c593
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c516
    loc_0x14000c516: // orphan
         dword [rdi + 0x20] = 8
         dword [rdi + 0x24] = 9
         ecx = 0x12
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_VariantClear
         ecx++                    // reloc.OLEAUT32.dll_VariantCopy
         r8 = ecx                 // reloc.OLEAUT32.dll_VariantCopy
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c548: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c514(x)
         qword [rdi] = r12
         
         goto loc_0x14000c54d
    loc_0x14000c54d: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c4ed(x)
         rdi = rsi

    loc_0x14000c550: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c54b(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.RAdmin] // [0x14006c918:8]=0x6d006400410052 // u"RAdmin"
         qword [rax] = xmm0
         ecx = dword [0x14006c920] // [0x14006c920:4]=0x6e0069 // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006c924] // [0x14006c924:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c5cb
    loc_0x14000c5cb: // orphan
         qword [rax] = r15
         rcx = r13 + 0x39
         qword [rax + 8] = rcx
         rax = rip + 0x8ffbc      // 0x14009c599
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c5f2
    loc_0x14000c5f2: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c624: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c5f0(x)
         qword [rdi] = r12
         
         goto loc_0x14000c629
    loc_0x14000c629: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c5c9(x)
         rdi = rsi

    loc_0x14000c62c: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c627(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 5
         dword [rbp - 0x3d] = 6
         ecx = 0xc
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.SSKey] // [0x14006c928:8]=0x65004b00530053 // u"SSKey"
         qword [rax] = xmm0
         ecx = dword [0x14006c930] // [0x14006c930:4]=121
         dword [rax + 8] = ecx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c69c
    loc_0x14000c69c: // orphan
         qword [rax] = r15
         rcx = r13 + 0x38
         qword [rax + 8] = rcx
         rax = rip + 0x8feea      // 0x14009c598
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c6c3
    loc_0x14000c6c3: // orphan
         dword [rdi + 0x20] = 5
         dword [rdi + 0x24] = 6
         ecx = 0xc
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // reloc.OLEAUT32.dll_SysStringLen
         r8 = ecx                 // reloc.OLEAUT32.dll_SysStringLen
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c6f5: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c6c1(x)
         qword [rdi] = r12
         
         goto loc_0x14000c6fa
    loc_0x14000c6fa: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c69a(x)
         rdi = rsi

    loc_0x14000c6fd: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c6f8(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.SCheck] // [0x14006c938:8]=0x65006800430053 // u"SCheck"
         qword [rax] = xmm0
         ecx = dword [0x14006c940] // [0x14006c940:4]=0x6b0063 // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006c944] // [0x14006c944:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c778
    loc_0x14000c778: // orphan
         qword [rax] = r15
         rcx = r13 + 0x4a
         qword [rax + 8] = rcx
         rax = rip + 0x8fe20      // 0x14009c5aa
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c79f
    loc_0x14000c79f: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c7d1: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c79d(x)
         qword [rdi] = r12
         
         goto loc_0x14000c7d6
    loc_0x14000c7d6: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c776(x)
         rdi = rsi

    loc_0x14000c7d9: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c7d4(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 7
         dword [rbp - 0x3d] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.AResKey] // [0x14006c948:16]=-1 // u"AResKey"
         xmmword [rax] = xmm0
         ecx = 0x28               // '(' // u(cstr 0x00000000) "AResKey"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c83e
    loc_0x14000c83e: // orphan
         qword [rax] = r15
         rcx = r13 + 0x4d
         qword [rax + 8] = rcx
         rax = rip + 0x8fd5d      // 0x14009c5ad
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c865
    loc_0x14000c865: // orphan
         dword [rdi + 0x20] = 7
         dword [rdi + 0x24] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // reloc.OLEAUT32.dll_VariantClear
         r8 = ecx                 // reloc.OLEAUT32.dll_VariantClear
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c897: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c863(x)
         qword [rdi] = r12
         
         goto loc_0x14000c89c
    loc_0x14000c89c: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c83c(x)
         rdi = rsi

    loc_0x14000c89f: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c89a(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.DisFOK] // [0x14006c958:8]=0x46007300690044 // u"DisFOK"
         qword [rax] = xmm0
         ecx = dword [0x14006c960] // [0x14006c960:4]=0x4b004f // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006c964] // [0x14006c964:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c91a
    loc_0x14000c91a: // orphan
         qword [rax] = r15
         rcx = r13 + 0x4e
         qword [rax + 8] = rcx
         rax = rip + 0x8fc82      // 0x14009c5ae
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000c941
    loc_0x14000c941: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000c973: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c93f(x)
         qword [rdi] = r12
         
         goto loc_0x14000c978
    loc_0x14000c978: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c918(x)
         rdi = rsi

    loc_0x14000c97b: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c976(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 0xb
         dword [rbp - 0x3d] = 0xc
         ecx = 0x18
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.CSuggestion] // [0x14006c968:16]=-1 // u"CSuggestion"
         xmmword [rax] = xmm0
         xmm1 = qword [0x14006c978] // [0x14006c978:8]=0x6e006f0069
         qword [rax + 0x10] = xmm1
         ecx = 0x28               // '(' // u(cstr 0x00000000) "CSuggest"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000c9ed
    loc_0x14000c9ed: // orphan
         qword [rax] = r15
         rdx = r13 + 0x4f
         qword [rax + 8] = rdx
         rax = rip + 0x8fbb0      // 0x14009c5af
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000ca14
    loc_0x14000ca14: // orphan
         dword [rdi + 0x20] = 0xb // [0xb:4]=0
         dword [rdi + 0x24] = 0xc // [0xc:4]=0
         ecx = 0x18
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000ca46: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000ca12(x)
         qword [rdi] = r12
         
         goto loc_0x14000ca4b
    loc_0x14000ca4b: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000c9eb(x)
         rdi = rsi

    loc_0x14000ca4e: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000ca49(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 0xc
         dword [rbp - 0x3d] = 0xd
         ecx = 0x1a
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.CSuggestion2] // [0x14006c980:16]=-1 // u"CSuggestion2"
         xmmword [rax] = xmm0
         xmm1 = qword [0x14006c990] // [0x14006c990:8]=0x32006e006f0069
         qword [rax + 0x10] = xmm1
         ecx = word [0x14006c998] // [0x14006c998:2]=0
         word [rax + 0x18] = cx
         ecx = 0x28               // '(' // u(cstr 0x00000000) "CSuggest"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000cacb
    loc_0x14000cacb: // orphan
         qword [rax] = r15
         rdx = r13 + 0x50
         qword [rax + 8] = rdx
         rax = rip + 0x8fad3      // 0x14009c5b0
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000caf2
    loc_0x14000caf2: // orphan
         dword [rdi + 0x20] = 0xc // [0xc:4]=0
         dword [rdi + 0x24] = 0xd // [0xd:4]=0xb0000000
         ecx = 0x1a
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000cb24: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000caf0(x)
         qword [rdi] = r12
         
         goto loc_0x14000cb29
    loc_0x14000cb29: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cac9(x)
         rdi = rsi

    loc_0x14000cb2c: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cb27(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 7
         dword [rbp - 0x3d] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.SModern] // [0x14006c9a0:16]=-1 // u"SModern"
         xmmword [rax] = xmm0
         ecx = 0x28               // '(' // u(cstr 0x00000000) "SModern"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000cb91
    loc_0x14000cb91: // orphan
         qword [rax] = r15
         rcx = r13 + 0x3b
         qword [rax + 8] = rcx
         rax = rip + 0x8f9f8      // 0x14009c59b
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000cbb8
    loc_0x14000cbb8: // orphan
         dword [rdi + 0x20] = 7
         dword [rdi + 0x24] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // reloc.OLEAUT32.dll_VariantClear
         r8 = ecx                 // reloc.OLEAUT32.dll_VariantClear
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000cbea: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cbb6(x)
         qword [rdi] = r12
         
         goto loc_0x14000cbef
    loc_0x14000cbef: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cb8f(x)
         rdi = rsi

    loc_0x14000cbf2: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cbed(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 4
         dword [rbp - 0x3d] = 5
         ecx = 0xa
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.Lang]  // [0x14006c9b0:8]=0x67006e0061004c // u"Lang"
         qword [rax] = xmm0
         ecx = word [0x14006c9b8] // [0x14006c9b8:2]=0
         word [rax + 8] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000cc64
    loc_0x14000cc64: // orphan
         qword [rax] = r15
         rcx = r13 + 0x3c
         qword [rax + 8] = rcx
         rax = rip + 0x8f926      // 0x14009c59c
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000cc8b
    loc_0x14000cc8b: // orphan
         dword [rdi + 0x20] = 4
         dword [rdi + 0x24] = 5
         ecx = 0xa
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000ccbd: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cc89(x)
         qword [rdi] = r12
         
         goto loc_0x14000ccc2
    loc_0x14000ccc2: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cc62(x)
         rdi = rsi

    loc_0x14000ccc5: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000ccc0(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 9
         dword [rbp - 0x3d] = 0xa
         ecx = 0x14
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.ICCharset] // [0x14006c9c0:16]=-1 // u"ICCharset"
         xmmword [rax] = xmm0
         ecx = dword [0x14006c9d0] // [0x14006c9d0:4]=116 // u(cstr 0x00000000) "ICCharse\uc59c\u4009\x01"
         dword [rax + 0x10] = ecx
         ecx = 0x28               // '(' // u(cstr 0x00000000) "ICCharset"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000cd33
    loc_0x14000cd33: // orphan
         qword [rax] = r15
         rcx = r13 + 0x40
         qword [rax + 8] = rcx
         rax = rip + 0x8f85b      // 0x14009c5a0
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000cd5a
    loc_0x14000cd5a: // orphan
         dword [rdi + 0x20] = 9
         dword [rdi + 0x24] = 0xa
         ecx = 0x14
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_VariantCopy
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000cd8c: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cd58(x)
         rax = rip + vtable.KeyValueType_unsigned_long_int_.0 // 0x14006d3f0
         qword [rdi] = rax
         
         goto loc_0x14000cd98
    loc_0x14000cd98: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cd31(x)
         rdi = rsi

    loc_0x14000cd9b: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cd96(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]        // vtable.KeyValueType_unsigned_long_int_.0
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 9
         dword [rbp - 0x3d] = 0xa
         ecx = 0x14
         fcn.14004bee0 ()
         r14 = rax                // vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.OCCharset] // [0x14006c9d8:16]=-1 // u"OCCharset"
         xmmword [rax] = xmm0
         ecx = dword [0x14006c9e8] // [0x14006c9e8:4]=116
         dword [rax + 0x10] = ecx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax                // u"OCCharset" vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000ce09
    loc_0x14000ce09: // orphan
         qword [rax] = r15
         rcx = r13 + 0x44
         qword [rax + 8] = rcx
         rax = rip + 0x8f789      // 0x14009c5a4
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000ce30
    loc_0x14000ce30: // orphan
         dword [rdi + 0x20] = 9
         dword [rdi + 0x24] = 0xa
         ecx = 0x14
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_VariantCopy
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000ce62: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000ce2e(x)
         rax = rip + vtable.KeyValueType_unsigned_long_int_.0 // 0x14006d3f0 // u"OCCharset"
         qword [rdi] = rax
         
         goto loc_0x14000ce6e
    loc_0x14000ce6e: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000ce07(x)
         rdi = rsi

    loc_0x14000ce71: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000ce6c(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]        // u"OCCharset" vtable.KeyValueType_unsigned_long_int_.0
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 8
         dword [rbp - 0x3d] = 9
         ecx = 0x12
         fcn.14004bee0 ()
         r14 = rax                // u"OCCharset" vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.CConvert] // [0x14006c9f0:16]=-1 // u"CConvert"
         xmmword [rax] = xmm0
         ecx = word [0x14006ca00] // [0x14006ca00:2]=0
         word [rax + 0x10] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax                // u"CConvert" vtable.KeyValueType_unsigned_long_int_.0
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000cee1
    loc_0x14000cee1: // orphan
         qword [rax] = r15
         rcx = r13 + 0x48
         qword [rax + 8] = rcx
         rax = rip + 0x8f6b5      // 0x14009c5a8
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000cf08
    loc_0x14000cf08: // orphan
         dword [rdi + 0x20] = 8
         dword [rdi + 0x24] = 9
         ecx = 0x12
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_VariantClear
         ecx++                    // reloc.OLEAUT32.dll_VariantCopy
         r8 = ecx                 // reloc.OLEAUT32.dll_VariantCopy
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000cf3a: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cf06(x)
         qword [rdi] = r12
         
         goto loc_0x14000cf3f
    loc_0x14000cf3f: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cedf(x)
         rdi = rsi

    loc_0x14000cf42: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cf3d(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 7
         dword [rbp - 0x3d] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.ToLower] // [0x14006ca08:16]=-1 // u"ToLower"
         xmmword [rax] = xmm0
         ecx = 0x28               // '(' // u(cstr 0x00000000) "ToLower"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000cfa7
    loc_0x14000cfa7: // orphan
         qword [rax] = r15
         rcx = r13 + 0x6a
         qword [rax + 8] = rcx
         rax = rip + 0x8f611      // 0x14009c5ca
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000cfce
    loc_0x14000cfce: // orphan
         dword [rdi + 0x20] = 7
         dword [rdi + 0x24] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // reloc.OLEAUT32.dll_VariantClear
         r8 = ecx                 // reloc.OLEAUT32.dll_VariantClear
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d000: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cfcc(x)
         qword [rdi] = r12
         
         goto loc_0x14000d005
    loc_0x14000d005: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000cfa5(x)
         rdi = rsi

    loc_0x14000d008: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d003(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 7
         dword [rbp - 0x3d] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.ToUpper] // [0x14006ca18:16]=-1 // u"ToUpper"
         xmmword [rax] = xmm0
         ecx = 0x28               // '(' // u(cstr 0x00000000) "ToUpper"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d06d
    loc_0x14000d06d: // orphan
         qword [rax] = r15
         rcx = r13 + 0x6b
         qword [rax + 8] = rcx
         rax = rip + 0x8f54c      // 0x14009c5cb
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d094
    loc_0x14000d094: // orphan
         dword [rdi + 0x20] = 7
         dword [rdi + 0x24] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // reloc.OLEAUT32.dll_VariantClear
         r8 = ecx                 // reloc.OLEAUT32.dll_VariantClear
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d0c6: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d092(x)
         qword [rdi] = r12
         
         goto loc_0x14000d0cb
    loc_0x14000d0cb: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d06b(x)
         rdi = rsi

    loc_0x14000d0ce: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d0c9(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 9
         dword [rbp - 0x3d] = 0xa
         ecx = 0x14
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.ToSenCase] // [0x14006ca28:16]=-1 // u"ToSenCase"
         xmmword [rax] = xmm0
         ecx = dword [0x14006ca38] // [0x14006ca38:4]=101 // u(cstr 0x00000000) "ToSenCas\uc5cb\u4009\x01"
         dword [rax + 0x10] = ecx
         ecx = 0x28               // '(' // u(cstr 0x00000000) "ToSenCase"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d13c
    loc_0x14000d13c: // orphan
         qword [rax] = r15
         rcx = r13 + 0x6c
         qword [rax + 8] = rcx
         rax = rip + 0x8f47e      // 0x14009c5cc
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d163
    loc_0x14000d163: // orphan
         dword [rdi + 0x20] = 9
         dword [rdi + 0x24] = 0xa
         ecx = 0x14
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_VariantCopy
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d195: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d161(x)
         qword [rdi] = r12
         
         goto loc_0x14000d19a
    loc_0x14000d19a: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d13a(x)
         rdi = rsi

    loc_0x14000d19d: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d198(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 0xd
         dword [rbp - 0x3d] = 0xe
         ecx = 0x1c
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.ToCapEachWord] // [0x14006ca40:16]=-1 // u"ToCapEachWord"
         xmmword [rax] = xmm0
         xmm1 = qword [0x14006ca50] // [0x14006ca50:8]=0x72006f00570068
         qword [rax + 0x10] = xmm1
         ecx = dword [0x14006ca58] // [0x14006ca58:4]=100 // u(cstr 0x00000000) "ToCapEac"
         dword [rax + 0x18] = ecx
         ecx = 0x28               // '(' // u(cstr 0x00000000) "ToCapEac"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d218
    loc_0x14000d218: // orphan
         qword [rax] = r15
         rdx = r13 + 0x6d
         qword [rax + 8] = rdx
         rax = rip + 0x8f3a3      // 0x14009c5cd
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d23f
    loc_0x14000d23f: // orphan
         dword [rdi + 0x20] = 0xd // [0xd:4]=0xcd000000
         dword [rdi + 0x24] = 0xe // [0xe:4]=0xc5cd0000
         ecx = 0x1c
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d271: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d23d(x)
         qword [rdi] = r12
         
         goto loc_0x14000d276
    loc_0x14000d276: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d216(x)
         rdi = rsi

    loc_0x14000d279: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d274(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 0xa
         dword [rbp - 0x3d] = 0xb
         ecx = 0x16
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.RemoveTone] // [0x14006ca60:16]=-1 // u"RemoveTone"
         xmmword [rax] = xmm0
         ecx = dword [0x14006ca70] // [0x14006ca70:4]=0x65006e // (pstr 0xffffffff) "\n"
         dword [rax + 0x10] = ecx
         ecx = word [0x14006ca74] // [0x14006ca74:2]=0
         word [rax + 0x14] = cx
         ecx = 0x28               // '(' // u(cstr 0x00000000) "RemoveTone"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d2f2
    loc_0x14000d2f2: // orphan
         qword [rax] = r15
         rcx = r13 + 0x6e
         qword [rax + 8] = rcx
         rax = rip + 0x8f2ca      // 0x14009c5ce
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d319
    loc_0x14000d319: // orphan
         dword [rdi + 0x20] = 0xa
         dword [rdi + 0x24] = 0xb // [0xb:4]=0
         ecx = 0x16
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d34b: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d317(x)
         qword [rdi] = r12
         
         goto loc_0x14000d350
    loc_0x14000d350: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d2f0(x)
         rdi = rsi

    loc_0x14000d353: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d34e(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.EMacro] // [0x14006ca78:8]=0x630061004d0045 // u"EMacro"
         qword [rax] = xmm0
         ecx = dword [0x14006ca80] // [0x14006ca80:4]=0x6f0072 // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006ca84] // [0x14006ca84:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d3ce
    loc_0x14000d3ce: // orphan
         qword [rax] = r15
         rcx = r13 + 0x49
         qword [rax + 8] = rcx
         rax = rip + 0x8f1c9      // 0x14009c5a9
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d3f5
    loc_0x14000d3f5: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d427: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d3f3(x)
         qword [rdi] = r12
         
         goto loc_0x14000d42c
    loc_0x14000d42c: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d3cc(x)
         rdi = rsi

    loc_0x14000d42f: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d42a(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.AUpper] // [0x14006ca88:8]=0x70007000550041 // u"AUpper"
         qword [rax] = xmm0
         ecx = dword [0x14006ca90] // [0x14006ca90:4]=0x720065 // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006ca94] // [0x14006ca94:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d4aa
    loc_0x14000d4aa: // orphan
         qword [rax] = r15
         rcx = r13 + 0x4c
         qword [rax + 8] = rcx
         rax = rip + 0x8f0f0      // 0x14009c5ac
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d4d1
    loc_0x14000d4d1: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d503: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d4cf(x)
         qword [rdi] = r12
         
         goto loc_0x14000d508
    loc_0x14000d508: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d4a8(x)
         rdi = rsi

    loc_0x14000d50b: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d506(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 8
         dword [rbp - 0x3d] = 9
         ecx = 0x12
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.CUnicode] // [0x14006ca98:16]=-1 // u"CUnicode"
         xmmword [rax] = xmm0
         ecx = word [0x14006caa8] // [0x14006caa8:2]=0
         word [rax + 0x10] = cx
         ecx = 0x28               // '(' // u(cstr 0x00000000) "CUnicode"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d57b
    loc_0x14000d57b: // orphan
         qword [rax] = r15
         rcx = r13 + 0x51
         qword [rax + 8] = rcx
         rax = rip + 0x8f024      // 0x14009c5b1
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d5a2
    loc_0x14000d5a2: // orphan
         dword [rdi + 0x20] = 8
         dword [rdi + 0x24] = 9
         ecx = 0x12
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_VariantClear
         ecx++                    // reloc.OLEAUT32.dll_VariantCopy
         r8 = ecx                 // reloc.OLEAUT32.dll_VariantCopy
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d5d4: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d5a0(x)
         qword [rdi] = r12
         
         goto loc_0x14000d5d9
    loc_0x14000d5d9: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d579(x)
         rdi = rsi

    loc_0x14000d5dc: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d5d7(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 7
         dword [rbp - 0x3d] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.AMarcro] // [0x14006cab0:16]=-1 // u"AMarcro"
         xmmword [rax] = xmm0
         ecx = 0x28               // '(' // u(cstr 0x00000000) "AMarcro"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d641
    loc_0x14000d641: // orphan
         qword [rax] = r15
         rcx = r13 + 0x52
         qword [rax + 8] = rcx
         rax = rip + 0x8ef5f      // 0x14009c5b2
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d668
    loc_0x14000d668: // orphan
         dword [rdi + 0x20] = 7
         dword [rdi + 0x24] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // reloc.OLEAUT32.dll_VariantClear
         r8 = ecx                 // reloc.OLEAUT32.dll_VariantClear
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d69a: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d666(x)
         qword [rdi] = r12
         
         goto loc_0x14000d69f
    loc_0x14000d69f: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d63f(x)
         rdi = rsi

    loc_0x14000d6a2: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d69d(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 4
         dword [rbp - 0x3d] = 5
         ecx = 0xa
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.RKey]  // [0x14006cac0:8]=0x790065004b0052 // u"RKey"
         qword [rax] = xmm0
         ecx = word [0x14006cac8] // [0x14006cac8:2]=0
         word [rax + 8] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d714
    loc_0x14000d714: // orphan
         qword [rax] = r15
         rcx = r13 + 0x18
         qword [rax + 8] = rcx
         rax = rip + 0x8ee52      // 0x14009c578
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d73b
    loc_0x14000d73b: // orphan
         dword [rdi + 0x20] = 4
         dword [rdi + 0x24] = 5
         ecx = 0xa
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d76d: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d739(x)
         rax = rip + vtable.KeyValueType_long_long___int64__.0 // 0x14006d3b0
         qword [rdi] = rax
         
         goto loc_0x14000d779
    loc_0x14000d779: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d712(x)
         rdi = rsi

    loc_0x14000d77c: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d777(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]        // vtable.KeyValueType_long_long___int64__.0
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 4
         dword [rbp - 0x3d] = 5
         ecx = 0xa
         fcn.14004bee0 ()
         r14 = rax                // vtable.KeyValueType_long_long___int64__.0
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.FKey]  // [0x14006cad0:8]=0x790065004b0046 // u"FKey"
         qword [rax] = xmm0
         ecx = word [0x14006cad8] // [0x14006cad8:2]=0
         word [rax + 8] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax                // vtable.KeyValueType_long_long___int64__.0
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d7ee
    loc_0x14000d7ee: // orphan
         qword [rax] = r15
         rcx = r13 + 0x28
         qword [rax + 8] = rcx
         rax = rip + 0x8ed88      // 0x14009c588
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d815
    loc_0x14000d815: // orphan
         dword [rdi + 0x20] = 4
         dword [rdi + 0x24] = 5
         ecx = 0xa
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d847: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d813(x)
         rax = rip + vtable.KeyValueType_long_long___int64__.0 // 0x14006d3b0
         qword [rdi] = rax
         
         goto loc_0x14000d853
    loc_0x14000d853: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d7ec(x)
         rdi = rsi

    loc_0x14000d856: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d851(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]        // vtable.KeyValueType_long_long___int64__.0
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax                // vtable.KeyValueType_long_long___int64__.0
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.NSound] // [0x14006cae0:8]=0x75006f0053004e // u"NSound"
         qword [rax] = xmm0
         ecx = dword [0x14006cae8] // [0x14006cae8:4]=0x64006e // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006caec] // [0x14006caec:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax                // vtable.KeyValueType_long_long___int64__.0
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d8d1
    loc_0x14000d8d1: // orphan
         qword [rax] = r15
         rcx = r13 + 0x55
         qword [rax + 8] = rcx
         rax = rip + 0x8ecd2      // 0x14009c5b5
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d8f8
    loc_0x14000d8f8: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d92a: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d8f6(x)
         qword [rdi] = r12
         
         goto loc_0x14000d92f
    loc_0x14000d92f: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d8cf(x)
         rdi = rsi

    loc_0x14000d932: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d92d(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 7
         dword [rbp - 0x3d] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.AUpdate] // [0x14006caf0:16]=-1 // u"AUpdate"
         xmmword [rax] = xmm0
         ecx = 0x28               // '(' // u(cstr 0x00000000) "AUpdate"
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000d997
    loc_0x14000d997: // orphan
         qword [rax] = r15
         rcx = r13 + 0x56
         qword [rax + 8] = rcx
         rax = rip + 0x8ec0d      // 0x14009c5b6
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000d9be
    loc_0x14000d9be: // orphan
         dword [rdi + 0x20] = 7
         dword [rdi + 0x24] = 8
         ecx = 0x10
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // reloc.OLEAUT32.dll_VariantClear
         r8 = ecx                 // reloc.OLEAUT32.dll_VariantClear
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000d9f0: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d9bc(x)
         qword [rdi] = r12
         
         goto loc_0x14000d9f5
    loc_0x14000d9f5: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d995(x)
         rdi = rsi

    loc_0x14000d9f8: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000d9f3(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 5
         dword [rbp - 0x3d] = 6
         ecx = 0xc
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.GMode] // [0x14006cb00:8]=0x64006f004d0047 // u"GMode"
         qword [rax] = xmm0
         ecx = dword [0x14006cb08] // [0x14006cb08:4]=101
         dword [rax + 8] = ecx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000da68
    loc_0x14000da68: // orphan
         qword [rax] = r15
         rcx = r13 + 0x63
         qword [rax + 8] = rcx
         rax = rip + 0x8eb49      // 0x14009c5c3
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000da8f
    loc_0x14000da8f: // orphan
         dword [rdi + 0x20] = 5
         dword [rdi + 0x24] = 6
         ecx = 0xc
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20]
         ecx++                    // reloc.OLEAUT32.dll_SysStringLen
         r8 = ecx                 // reloc.OLEAUT32.dll_SysStringLen
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000dac1: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000da8d(x)
         qword [rdi] = r12
         
         goto loc_0x14000dac6
    loc_0x14000dac6: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000da66(x)
         rdi = rsi

    loc_0x14000dac9: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000dac4(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         r14 = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.SConso] // [0x14006cb10:8]=0x6e006f00430053 // u"SConso"
         qword [rax] = xmm0
         ecx = dword [0x14006cb18] // [0x14006cb18:4]=0x6f0073 // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006cb1c] // [0x14006cb1c:2]=0
         word [rax + 0xc] = cx
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         rdi = rax
         qword [rbp - 0x39] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14000db44
    loc_0x14000db44: // orphan
         qword [rax] = r15
         rcx = r13 + 0x4b
         qword [rax + 8] = rcx
         rax = rip + 0x8ea55      // 0x14009c5ab
         qword [rdi + 0x10] = rax
         qword [rdi + 0x18] = rsi
         qword [rdi + 0x20] = 0
         v = r14 & r14
         if (!v) 
         goto loc_0x14000db6b
    loc_0x14000db6b: // orphan
         dword [rdi + 0x20] = 6
         dword [rdi + 0x24] = 7
         ecx = 0xe
         fcn.14004bee0 ()
         qword [rdi + 0x18] = rax
         ecx = dword [rdi + 0x20] // reloc.OLEAUT32.dll_SysStringLen
         ecx++
         r8 = ecx
         r8 += r8
         rdx = r14
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)

    loc_0x14000db9d: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000db69(x)
         qword [rdi] = r12
         
         goto loc_0x14000dba2
    loc_0x14000dba2: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000db42(x)
         rdi = rsi

    loc_0x14000dba5: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000dba0(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]
         qword [rax + rcx*8] = rdi
         qword [rbx + 8]++
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 9
         r14d = 0xa
         dword [rbp - 0x3d] = r14d
         ecx = r14 + 0xa
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.AutoStart] // [0x14006cb20:16]=-1 // u"AutoStart"
         xmmword [rax] = xmm0
         ecx = dword [0x14006cb30] // [0x14006cb30:4]=116 // u(cstr 0x00000000) "AutoStar\uc5ab\u4009\x01"
         dword [rax + 0x10] = ecx
         r8 = r13 + 0x68          // u(cstr 0x00000000) "AutoStart"
         r9 = rip + 0x8e9be       // 0x14009c5c8 // u(cstr 0x00000000) "AutoStart"
         rdx = rbp - 0x49         // u(pstr 0x00000000) "AutoStart"
         rcx = rbx
         fcn.14000a120 ()         // fcn.14000a120(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = r14 + 4            // u"rt"
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.CTIcon] // [0x14006cb38:8]=0x63004900540043 // u"CTIcon"
         qword [rax] = xmm0
         ecx = dword [0x14006cb40] // [0x14006cb40:4]=0x6e006f // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006cb44] // [0x14006cb44:2]=0
         word [rax + 0xc] = cx
         r8 = r13 + 0x6f
         r9 = rip + 0x8e967       // 0x14009c5cf
         rdx = rbp - 0x49
         rcx = rbx
         fcn.14000a120 ()         // fcn.14000a120(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = r14 + 4            // u"rt"
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.EColor] // [0x14006cb48:8]=0x6c006f00430045 // u"EColor"
         qword [rax] = xmm0
         ecx = dword [0x14006cb50] // [0x14006cb50:4]=0x72006f // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006cb54] // [0x14006cb54:2]=0
         word [rax + 0xc] = cx
         r8 = r13 + 0x70
         r9 = rip + 0x8e90a       // 0x14009c5d0
         rdx = rbp - 0x49
         rcx = rbx
         fcn.14000a1b0 ()         // fcn.14000a1b0(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 7
         dword [rbp - 0x3d] = 8
         r15d = 0x10
         ecx = r15d
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.EBColor] // [0x14006cb58:16]=-1 // u"EBColor"
         xmmword [rax] = xmm0
         r8 = r13 + 0x74          // u(cstr 0x00000000) "EBColor"
         r9 = rip + 0x8e8c1       // 0x14009c5d4 // u(cstr 0x00000000) "EBColor"
         rdx = rbp - 0x49         // u(pstr 0x00000000) "EBColor"
         rcx = rbx
         fcn.14000a1b0 ()         // fcn.14000a1b0(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = r14 + 4
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.VColor] // [0x14006cb68:8]=0x6c006f00430056 // u"VColor"
         qword [rax] = xmm0
         ecx = dword [0x14006cb70] // [0x14006cb70:4]=0x72006f // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006cb74] // [0x14006cb74:2]=0
         word [rax + 0xc] = cx
         r8 = r13 + 0x78
         r9 = rip + 0x8e867       // 0x14009c5d8
         rdx = rbp - 0x49
         rcx = rbx
         fcn.14000a1b0 ()         // fcn.14000a1b0(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 7
         dword [rbp - 0x3d] = 8
         ecx = r15d
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.VBColor] // [0x14006cb78:16]=-1 // u"VBColor"
         xmmword [rax] = xmm0
         r8 = r13 + 0x7c          // u(cstr 0x00000000) "VBColor"
         r9 = rip + 0x8e824       // 0x14009c5dc // u(cstr 0x00000000) "VBColor"
         rdx = rbp - 0x49         // u(pstr 0x00000000) "VBColor"
         rcx = rbx
         fcn.14000a1b0 ()         // fcn.14000a1b0(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 5
         dword [rbp - 0x3d] = 6
         ecx = r14 + 2
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.NPCAM] // [0x14006cb88:8]=0x4100430050004e // u"NPCAM"
         qword [rax] = xmm0
         edx = dword [0x14006cb90] // [0x14006cb90:4]=77
         dword [rax + 8] = edx
         r8 = r13 + 0x80
         r9 = rip + 0x8e7d2       // 0x14009c5e0
         rdx = rbp - 0x49
         rcx = rbx
         fcn.14000a120 ()         // fcn.14000a120(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 8
         dword [rbp - 0x3d] = 9
         ecx = r14 + 8
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.DefSound] // [0x14006cb98:16]=-1 // u"DefSound"
         xmmword [rax] = xmm0
         edx = word [0x14006cba8] // [0x14006cba8:2]=0
         word [rax + 0x10] = dx
         r8 = r13 + 0x81
         r9 = rip + 0x8e77d       // 0x14009c5e1 // u(cstr 0x00000000) "DefSound"
         rdx = rbp - 0x49         // u(pstr 0x00000000) "DefSound"
         rcx = rbx
         fcn.14000a120 ()         // fcn.14000a120(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = r14d
         dword [rbp - 0x3d] = 0xb
         ecx = r14 + 0xc          // u(pstr 0x00000000) "DefSound"
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.ThemeColor] // [0x14006cbb0:16]=-1 // u"ThemeColor"
         xmmword [rax] = xmm0
         ecx = dword [0x14006cbc0] // [0x14006cbc0:4]=0x72006f // (pstr 0xffffffff) "\n"
         dword [rax + 0x10] = ecx
         edx = word [0x14006cbc4] // [0x14006cbc4:2]=0
         word [rax + 0x14] = dx
         r8 = r13 + 0x82          // u(pstr 0x00000000) "ThemeColor"
         r9 = rip + 0x8e722       // 0x14009c5e2 // u(cstr 0x00000000) "ThemeColor"
         rdx = rbp - 0x49         // u(pstr 0x00000000) "ThemeColor"
         rcx = rbx
         fcn.14000a120 ()         // fcn.14000a120(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 6
         dword [rbp - 0x3d] = 7
         ecx = r14 + 4            // u"lor"
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.UseIME] // [0x14006cbc8:8]=0x49006500730055 // u"UseIME"
         qword [rax] = xmm0
         ecx = dword [0x14006cbd0] // [0x14006cbd0:4]=0x45004d // (pstr 0xffffffff) "\n"
         dword [rax + 8] = ecx
         ecx = word [0x14006cbd4] // [0x14006cbd4:2]=0
         word [rax + 0xc] = cx
         r8 = r13 + 0x53
         r9 = rip + 0x8e695       // 0x14009c5b3
         rdx = rbp - 0x49
         rcx = rbx
         fcn.14000a120 ()         // fcn.14000a120(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 9
         dword [rbp - 0x3d] = r14d
         ecx = r14 + 0xa
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.InputMech] // [0x14006cbd8:16]=-1 // u"InputMech"
         xmmword [rax] = xmm0
         edx = dword [0x14006cbe8] // [0x14006cbe8:4]=104 // u(cstr 0x00000000) "InputMecor"
         dword [rax + 0x10] = edx
         r8 = r13 + 0x54          // u(cstr 0x00000000) "InputMech"
         r9 = rip + 0x8e648       // 0x14009c5b4 // u(cstr 0x00000000) "InputMech"
         rdx = rbp - 0x49         // u(pstr 0x00000000) "InputMech"
         rcx = rbx
         fcn.14000a120 ()         // fcn.14000a120(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         dword [rbp - 0x41] = 9
         dword [rbp - 0x3d] = r14d
         ecx = r14 + 0xa          // u(pstr 0x00000000) "InputMech"
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = xmmword [str.UseSpCase] // [0x14006cbf0:16]=-1 // u"UseSpCase"
         xmmword [rax] = xmm0
         ecx = dword [0x14006cc00] // [0x14006cc00:4]=101 // u(cstr 0x00000000) "UseSpCash"
         dword [rax + 0x10] = ecx
         r8 = r13 + 0xa8          // u(cstr 0x00000000) "UseSpCase"
         r9 = rip + 0x8e64b       // 0x14009c608
         rdx = rbp - 0x49         // u(pstr 0x00000000) "UseSpCase"
         rcx = rbx
         fcn.14000a120 ()         // fcn.14000a120(0x0, 0x0, 0x177fb7, 0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         r15d = esi
         qword [rbp - 0x29] = rsi

    loc_0x14000dfe0: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000e17f(x)
         r15d++
         dword [rbp - 0x31] = r15d
         r9d = r14d
         r8d = 0x20               // u(pstr 0x00000006) "SpCase"
         rdx = rbp - 0x19
         ecx = r15d
         fcn.14004c168 ()         // fcn.14004c168(0x0, 0x0, 0x177fe7)
         r12 = rsi
         rax = rbp - 0x19
         r14 = 0xffffffffffffffff

    loc_0x14000e010: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000e019(x)
         r14++
         v = word [rax + r14*2] - 0
         if (v) 
         goto loc_0x14000e01b
    loc_0x14000e01b: // orphan
         v = r14d & r14d
         if (v <= 0) 
         goto loc_0x14000e020
    loc_0x14000e020: // orphan
         eax = r14 + 1
         rdi = eax
         rdi += rdi               // u"seSpCase" reloc.OLEAUT32.dll_SysReAllocString
         rcx = rdi                // u"seSpCase" reloc.OLEAUT32.dll_SysReAllocString
         fcn.14004bee0 ()
         r12 = rax
         r8 = rdi                 // u"seSpCase" reloc.OLEAUT32.dll_SysReAllocString
         rdx = rbp - 0x19
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x2, 0x0, 0x177fe7)

    loc_0x14000e044: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000e01e(x)
         dword [rbp - 0x41] = 4
         dword [rbp - 0x3d] = 5
         ecx = 0xa                // u"Case"
         fcn.14004bee0 ()
         rdi = rax
         qword [rbp - 0x49] = rax
         xmm0 = qword [str.CASF]  // [0x14006cc08:8]=0x46005300410043 // u"CASF"
         qword [rax] = xmm0
         ecx = word [0x14006cc10] // [0x14006cc10:2]=0
         word [rax + 8] = cx
         rcx = rip + 0x5cf37      // 0x14006afb8
         v = r12 & r12
         if (!zf) rcx = r12
         qword [rbp - 0x39] = rcx
         v = r14d & r14d
         if (v <= 0) 
         goto loc_0x14000e091
    loc_0x14000e091: // orphan
         r15d = r14 + 5
         v = r15d - 5
         if (v <= 0) 
         goto loc_0x14000e09b
    loc_0x14000e09b: // orphan
         v = r15d - 0xa
         eax = 0xa                // u"Case"
         cmovle r15d eax          // u"Case"
         v = r15d - 0x10
         eax = 0x10
         if (v < 0) r15d = eax
         rdx = r15d
         rdx += rdx
         rcx = rdi
         fcn.14004bee8 ()
         v = rax & rax
         if (!v) 
    loc_0x14000e0c8: // orphan
         rdi = rax
         qword [rbp - 0x49] = rax
         dword [rbp - 0x3d] = r15d

    loc_0x14000e0d3: // orphan
         // CODE XREFS from fcn.14000bcc0 @ 0x14000e099(x), 0x14000e0c6(x)
         r8 = r14d
         r8 += r8
         rcx = rdi + 8            // reloc.OLEAUT32.dll_VariantClear
         rdx = qword [rbp - 0x39]
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x14006afb8)
         eax = r14 + 4
         dword [rbp - 0x41] = eax
         rax = r14d
         word [rdi + rax*2 + 8] = si
         r15d = dword [rbp - 0x31]

    loc_0x14000e0f9: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000e08f(x)
         rcx = r12
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         ecx = 0x28               // '('
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         r14 = rax
         qword [rbp - 0x39] = rax
         r12 = qword [rbp - 0x29]
         v = rax & rax
         if (!v) 
         goto loc_0x14000e11b
    loc_0x14000e11b: // orphan
         r9 = rip + 0x8e495       // 0x14009c5b7
         r9 += r12
         r8 = r12 + 0x57
         r8 += r13
         rdx = rbp - 0x49
         rcx = rax
         fcn.14000a830 ()         // fcn.14000a830(0x0, 0x0, 0x177fb7, 0x0)
         rax = rip + vtable.KeyValueType_unsigned_char_.0 // 0x14006d430
         qword [r14] = rax
         
         goto loc_0x14000e145
    loc_0x14000e145: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000e119(x)
         r14 = rsi

    loc_0x14000e148: // orphan
         // CODE XREF from fcn.14000bcc0 @ 0x14000e143(x)
         rdx = qword [rbx + 8]
         rdx++
         rcx = rbx
         fcn.14000b130 ()         // fcn.14000b130(0x0, 0x0)
         rcx = qword [rbx + 8]
         rax = qword [rbx]        // vtable.KeyValueType_unsigned_char_.0
         qword [rax + rcx*8] = r14
         qword [rbx + 8]++
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         r12++
         qword [rbp - 0x29] = r12
         v = r15d - 0xc
         r14d = 0xa               // u"Case"
         jl 0x14000dfe0           // likely

         goto loc_0x14000e185
    loc_0x14000e185: // orphan
         rcx = qword [rbp + 0x27]
         rcx ^= rsp
         fcn.140047380 ()         // fcn.140047380(0x0)
         r11 = var_a0h            // (pstr 0xffffffff) "\n"
         rbx = qword [r11 + 0x30] // (pstr 0xffffffff) "\n"
         rsi = qword [r11 + 0x40]
         rdi = qword [r11 + 0x48]
         rsp = r11                // (pstr 0xffffffff) "\n"
         r15 = pop ()             // "\n"
         r14 = pop ()             // (pstr 0xffffffff) "\n"
         r13 = pop ()             // "\n"
         r12 = pop ()
         rbp = pop ()
         return

        return rax;
}

```

## tray_controller

```c
===== FUNCTION tray_controller 0x140016ec0 =====
[{"addr":5368803008,"name":"fcn.140016ec0","size":312,"is-pure":"false","realsz":312,"noreturn":false,"recursive":false,"stackframe":1072,"calltype":"ms","cost":100,"cc":7,"bits":64,"type":"fcn","nbbs":12,"tracecov":0,"is-lineal":true,"ninstrs":68,"edges":17,"ebbs":1,"signature":"fcn.140016ec0 ();","minaddr":5368803008,"maxaddr":5368803320,"maxbbins":13,"midbbins":5.667,"ratbbins":2.294,"indegree":11,"outdegree":9,"nlocals":8,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.140016ec0 (int rcx, int rdx) {
    loc_0x140016ec0:
        // XREFS: CALL 0x140019ac6  CALL 0x140023ebb  CALL 0x14002408a
        // XREFS: CALL 0x140024115  CALL 0x140024254  CALL 0x140032afd
        // XREFS: CALL 0x14003376c  CALL 0x140033b20  CALL 0x140033bd3
        // XREFS: CALL 0x1400353f0  CALL 0x14003540c
        push (rbx)
        rsp -= 0x400
        rax = qword [0x14009a9e8] // [0x14009a9e8:8]=0x2b992ddfa200
        rax ^= rsp
        qword [var_3f0h] = rax // [0x3f0:8]=-1
        rax = qword [0x14009c860] // [0x14009c860:8]=0
        ebx = cl
        v = rax & rax
        if (!v) goto loc_0x140016fdf // likely
        goto loc_0x140016eee;
    loc_0x140016fdf:
        // CODE XREF from fcn.140016ec0 @ 0x140016ee8(x)
        rcx = qword [var_3f0h]
        rcx ^= rsp
        fcn.140047380 () // fcn.140047380(0x0)
        rsp += 0x400
        rbx = pop ()
        return
        goto loc_0x140016f26;
        return rax;
    loc_0x140016f26:
        fcn.140018040 ()
        v = byte [0x14009c9bc] - 0 // [0x14009c9bc:1]=0
        rcx = var_48h
        qword [var_40h] = rax
        if (!v) goto loc_0x140016f47 // likely
        goto loc_0x140016f3e;
    loc_0x140016f47:
        // CODE XREF from fcn.140016ec0 @ 0x140016f3c(x)
        rdx = rip + 0x56cf2 // u"EVKey - Click to turn off Vietnamese mode" // 0x14006dc40
        goto loc_0x140016f76
        
    loc_0x140016f76:
        // CODE XREFS from fcn.140016ec0 @ 0x140016f45(x), 0x140016f4e(x), 0x140016f6d(x)
        qword [sym.imp.KERNEL32.dll_lstrcpyW] () // [0x14006a4e0:8]=0x919c8 reloc.KERNEL32.dll_lstrcpyW // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_lstrcpyW // LPWSTR lstrcpyW(0, 0)
        rdx = qword [0x14009c860] // [0x14009c860:8]=0
        ebx = 0
        dword [var_34h] = 7
        ecx = ebx    // DWORD dwMessage
        eax = dword [rdx + 0x860]
        dword [var_38h] = eax
        v = dword [rdx + 8] - ebx
        rdx = lpData // PNOTIFYICONDATAW lpData
        cl = v != 0
        qword [sym.imp.SHELL32.dll_Shell_NotifyIconW] () // [0x14006a600:8]=0x927e0 reloc.SHELL32.dll_Shell_NotifyIconW // (pstr 0xffffffff) "\n" reloc.SHELL32.dll_Shell_NotifyIconW // BOOL Shell_NotifyIconW(0, 0x00000000000003d0)
        v = eax & eax
        if (v) goto loc_0x140016fcf // likely
        goto loc_0x140016fae;
        return rax;
    loc_0x140016f3e: // orphan
         rdx = rip + 0x56cab      // u"EVKey - K\xedch \u0111\u1ec3 t\u1eaft b\xe0n ph\xedm Ti\u1ebfng Vi\u1ec7t" // 0x14006dbf0
         
    loc_0x140016f50: // orphan
         // CODE XREF from fcn.140016ec0 @ 0x140016f24(x)
         fcn.140016100 ()
         v = byte [0x14009c9bc] - 0 // [0x14009c9bc:1]=0
         rcx = var_48h
         qword [var_40h] = rax
         rdx = rip + str.EVKey___K // 0x14006dca0 // u"EVKey - K\xedch \u0111\u1ec3 m\u1edf b\xe0n ph\xedm Ti\u1ebfng Vi\u1ec7t"
         if (v) 
         goto loc_0x140016f6f
    loc_0x140016f6f: // orphan
         rdx = rip + str.EVKey___Click_to_turn_on_Vietnamese_mode // 0x14006dcf0 // u"EVKey - Click to turn on Vietnamese mode"

    loc_0x140016f76: // orphan
         // CODE XREFS from fcn.140016ec0 @ 0x140016f45(x), 0x140016f4e(x), 0x140016f6d(x)
         qword [sym.imp.KERNEL32.dll_lstrcpyW] () // [0x14006a4e0:8]=0x919c8 reloc.KERNEL32.dll_lstrcpyW // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_lstrcpyW // LPWSTR lstrcpyW(0, 0)
         rdx = qword [0x14009c860] // [0x14009c860:8]=0
         ebx = 0
         dword [var_34h] = 7
         ecx = ebx                // DWORD dwMessage
         eax = dword [rdx + 0x860]
         dword [var_38h] = eax
         v = dword [rdx + 8] - ebx
         rdx = lpData             // PNOTIFYICONDATAW lpData
         cl = v != 0
         qword [sym.imp.SHELL32.dll_Shell_NotifyIconW] () // [0x14006a600:8]=0x927e0 reloc.SHELL32.dll_Shell_NotifyIconW // (pstr 0xffffffff) "\n" reloc.SHELL32.dll_Shell_NotifyIconW // BOOL Shell_NotifyIconW(0, 0x00000000000003d0)
         v = eax & eax
         if (v) 
         goto loc_0x140016fae
    loc_0x140016fae: // orphan
         rdx = lpData             // PNOTIFYICONDATAW lpData
         ecx = 0                  // DWORD dwMessage
         qword [sym.imp.SHELL32.dll_Shell_NotifyIconW] () // [0x14006a600:8]=0x927e0 reloc.SHELL32.dll_Shell_NotifyIconW // (pstr 0xffffffff) "\n" reloc.SHELL32.dll_Shell_NotifyIconW // BOOL Shell_NotifyIconW(0, 0x00000000000003d0)
         v = eax & eax
         if (v) 
    loc_0x140016fbf: // orphan
         rdx = lpData             // PNOTIFYICONDATAW lpData
         ecx = rbx + 1
         qword [sym.imp.SHELL32.dll_Shell_NotifyIconW] () // [0x14006a600:8]=0x927e0 reloc.SHELL32.dll_Shell_NotifyIconW // (pstr 0xffffffff) "\n" reloc.SHELL32.dll_Shell_NotifyIconW // BOOL Shell_NotifyIconW(0x014006d4, 0x00000000000003d0)
         v = eax & eax

    loc_0x140016fcf: // orphan
         // CODE XREFS from fcn.140016ec0 @ 0x140016fac(x), 0x140016fbd(x)
         rcx = qword [0x14009c860] // [0x14009c860:8]=0
         bl = v != 0
         eax = bl
         dword [rcx + 8] = ebx

}

```

## update_controller

```c
===== FUNCTION update_controller 0x1400178e0 =====
[{"addr":5368805600,"name":"fcn.1400178e0","size":1165,"is-pure":"false","realsz":1165,"noreturn":false,"recursive":false,"stackframe":304,"calltype":"ms","cost":405,"cc":17,"bits":64,"type":"fcn","nbbs":33,"tracecov":0,"is-lineal":true,"ninstrs":316,"edges":48,"ebbs":1,"signature":"fcn.1400178e0 (int64_t arg1);","minaddr":5368805600,"maxaddr":5368806765,"maxbbins":49,"midbbins":9.576,"ratbbins":5.117,"indegree":0,"outdegree":33,"nlocals":3,"nargs":1,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.1400178e0 (int64_t arg1) {
    loc_0x1400178e0:
        // DATA XREF from fcn.140017480 @ 0x1400175b5(r)
        rax = rsp
        push (rbp)
        push (r12)
        push (r13)
        push (r14)
        push (r15)
        rbp = rax - 0x5f
        rsp -= 0xe0
        qword [rbp - 0x31] = 0xfffffffffffffffe
        qword [rax + 0x10] = rbx
        qword [rax + 0x18] = rsi
        qword [rax + 0x20] = rdi
        rax = qword [0x14009a9e8] // [0x14009a9e8:8]=0x2b992ddfa200
        rax ^= rsp
        qword [rbp + 0x27] = rax
        r12 = rcx    // arg1
        rcx = rbp - 0x51
        fcn.140016270 () // fcn.140016270(0x0)
        rcx = qword [rax]
        rdx = rip + 0x53688 // 0x14006afb8
        v = rcx & rcx
        if (!zf) rdx = rcx
        rcx = rbp - 0x79
        fcn.140038400 () // fcn.140038400(0x0, 0x0)
        rcx = qword [rbp - 0x51]
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        edx = dword [rbp - 0x71] // (pstr 0xffffffff) "\n"
        edx += 0xb   // (pstr 0xffffffff) "\n"
        rcx = rbp - 0x79
        fcn.140008a10 () // fcn.140008a10(0x0, 0x0)
        rcx = dword [rbp - 0x71] // (pstr 0xffffffff) "\n"
        rax = qword [rbp - 0x79]
        rdx = rax + rcx*2
        xmm0 = xmmword [str.evkau.exe] // [0x14006d5f0:16]=-1
        xmmword [rdx] = xmm0
        eax = dword [0x14006d600] // [0x14006d600:4]=0x650078 // (pstr 0xffffffff) "\n"
        dword [rdx + 0x10] = eax
        eax = word [0x14006d604] // [0x14006d604:2]=0
        word [rdx + 0x14] = ax
        eax = dword [rbp - 0x71] // (pstr 0xffffffff) "\n"
        eax += 0xa   // (pstr 0xffffffff) "\n"
        dword [rbp - 0x71] = eax
        r13d = 0
        qword [rbp - 0x69] = r13
        qword [rbp - 0x61] = r13
        v = eax & eax
        if (v <= 0) goto loc_0x1400179cb // unlikely
        goto loc_0x14001799b;
    loc_0x1400179cb:
        // CODE XREFS from fcn.1400178e0 @ 0x140017999(x), 0x14001799f(x)
        rcx = qword [rbp - 0x79]
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        qword [rbp - 0x51] = r13
        qword [rbp - 0x49] = r13
        rdx = rbp - 0x51
        rcx = rbp - 0x69
        fcn.140016740 () // fcn.140016740(0x0, 0x0)
        rbx = qword [rbp - 0x69]
        v = eax - 0x190
        if (v >= 0) goto loc_0x140017a22 // unlikely
        goto loc_0x1400179f4;
    loc_0x140017a22:
        // CODE XREF from fcn.1400178e0 @ 0x1400179f2(x)
        rsi = r13
        qword [rbp - 0x79] = r13
        edi = r13d
        r14d = r13d
        dword [rbp - 0x6d] = r13d
        eax = r13d
        ecx = r13d
        edx = 0x10
        r15d = rdx - 0xd
        v = byte [r12] - r13b
        if (!v) goto loc_0x140017a8b // unlikely
        goto loc_0x140017a48;
    loc_0x140017a8b:
        // CODE XREF from fcn.1400178e0 @ 0x140017a46(x)
        v = byte [r12 + 1] - 0
        if (!v) goto loc_0x140017ade // unlikely
        goto loc_0x140017a93;
    loc_0x140017ade:
        // CODE XREF from fcn.1400178e0 @ 0x140017a91(x)
        edx = 2      // reloc.OLEAUT32.dll_SysReAllocString
        rcx = r12
        fcn.1400474e4 ()
        r12d = rdi + 5
        dword [rbp - 0x59] = r12d
        ebx = r12 + 1 // reloc.OLEAUT32.dll_SysStringLen
        v = ebx - r14d
        if (v <= 0) goto loc_0x140017b31 // unlikely
        goto loc_0x140017afd;
    loc_0x140017b31:
        // CODE XREFS from fcn.1400178e0 @ 0x140017afb(x), 0x140017b22(x)
        rax = edi
        rcx = rsi + rax*2
        xmm0 = qword [0x14006d758] // [0x14006d758:8]=0x360066002d0020
        qword [rcx] = xmm0
        eax = dword [0x14006d760] // [0x14006d760:4]=52
        dword [rcx + 8] = eax
        dword [rbp - 0x71] = r12d
        qword [rbp - 0x41] = r13
        qword [rbp - 0x39] = 0
        rcx = rbp - 0x29
        fcn.140016270 () // fcn.140016270(0x0)
        rdx = rbp - 0x41
        rcx = rax
        fcn.140016740 () // fcn.140016740(0x0, 0x0)
        r9d = 0xa
        r8d = r9 + 0x16
        rdx = rbp - 0x19
        ecx = eax
        fcn.14004c168 () // fcn.14004c168(0x0, 0x0, 0x177fe7)
        qword [rbp - 0x79] = r13
        rax = rbp - 0x19
        rdi = 0xffffffffffffffff
        
    loc_0x140017ba0:
        // CODE XREF from fcn.1400178e0 @ 0x140017ba8(x)
        rdi++
        v = word [rax + rdi*2] - 0
        if (v) goto loc_0x140017ba0 // unlikely
        goto loc_0x140017baa;
        goto loc_0x140017b24;
        goto loc_0x140017a9a;
        goto loc_0x140017a5f;
        goto loc_0x1400179a1;
        return rax;
    loc_0x1400179a1:
        dword [rbp - 0x61] = eax
        eax++
        dword [rbp - 0x5d] = eax
        rbx = eax
        rbx += rbx   // reloc.OLEAUT32.dll_SysReAllocString
        rcx = rbx    // reloc.OLEAUT32.dll_SysReAllocString
        fcn.14004bee0 ()
        qword [rbp - 0x69] = rax
        r8 = rbx     // reloc.OLEAUT32.dll_SysReAllocString
        rdx = qword [rbp - 0x79]
        rcx = rax
        fcn.140049380 () // fcn.140049380(0x0, 0x0, 0x0)
    loc_0x1400179f4: // orphan
         edx = 2                  // reloc.OLEAUT32.dll_SysReAllocString
         rcx = r12
         fcn.1400474e4 ()
         r9d = 0x30               // '0' // UINT uType
         r8 = rip + str.EVKey___Auto_Update // 0x14006d608 // u"EVKe" // LPCWSTR lpCaption
         rdx = rip + 0x55c1b      // u"Ch\u01b0\u01a1ng tr\xecnh \"T\u1ef1 \u0111\u1ed9ng c\u1eadp nh\u1eadt\" hi\u1ec7n \u0111ang l\xe0 phi\xean b\u1ea3n c\u0169.\nB\u1ea1n " // 0x14006d630 // LPCWSTR lpText
         ecx = 0                  // HWND hWnd
         qword [sym.imp.USER32.dll_MessageBoxW] () // [0x14006aa48:8]=0x91f90 reloc.USER32.dll_MessageBoxW // (pstr 0xffffffff) "\n" reloc.USER32.dll_MessageBoxW // int MessageBoxW(0, "C", "E", 0x00000000)
         
    loc_0x140017a48: // orphan
         edx = r15 + 0x1d
         fcn.14004bee8 ()
         rdx = rax
         eax = r13d
         ecx = r13d
         v = rdx & rdx
         if (!v) 
         goto loc_0x140017a5f
    loc_0x140017a5f: // orphan
         rsi = rdx
         qword [rbp - 0x79] = rdx
         edx = 0x10
         r14d = edx
         dword [rbp - 0x6d] = edx
         eax = edx
         ecx = edx
         
         goto loc_0x140017a77
    loc_0x140017a77: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017a5d(x)
         edx = 0x10

    loc_0x140017a7c: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017a75(x)
         xmm0 = qword [0x14006d748] // [0x14006d748:8]=0x70002d0020
         qword [rsi] = xmm0
         edi = r15d

    loc_0x140017a93: // orphan
         ebx = rdi + 4
         v = ebx - eax
         if (v <= 0) 
         goto loc_0x140017a9a
    loc_0x140017a9a: // orphan
         eax = rcx + rcx
         v = eax - ebx
         cmovge ebx eax
         v = ebx - 0x10
         if (v < 0) ebx = edx
         rdx = ebx
         rdx += rdx
         rcx = rsi
         fcn.14004bee8 ()
         v = rax & rax
         if (!v) 
    loc_0x140017abb: // orphan
         rsi = rax
         qword [rbp - 0x79] = rax
         r14d = ebx
         dword [rbp - 0x6d] = ebx

    loc_0x140017ac8: // orphan
         // CODE XREFS from fcn.1400178e0 @ 0x140017a98(x), 0x140017ab9(x)
         rax = edi
         xmm0 = qword [0x14006d750] // [0x14006d750:8]=0x73002d0020
         qword [rsi + rax*2] = xmm0
         edi += r15d
         r13d = 0

    loc_0x140017afd: // orphan
         eax = r14 + r14
         v = eax - ebx
         cmovge ebx eax
         v = ebx - 0x10
         eax = 0x10
         if (v < 0) ebx = eax
         rdx = ebx
         rdx += rdx
         rcx = rsi
         fcn.14004bee8 ()
         v = rax & rax
         if (!v) 
         goto loc_0x140017b24
    loc_0x140017b24: // orphan
         rsi = rax
         qword [rbp - 0x79] = rax
         r14d = ebx
         dword [rbp - 0x6d] = ebx

    loc_0x140017ba0: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017ba8(x)
         rdi++
         v = word [rax + rdi*2] - 0
         if (v) 
         goto loc_0x140017baa
    loc_0x140017baa: // orphan
         v = edi & edi
         if (v <= 0) 
         goto loc_0x140017bae
    loc_0x140017bae: // orphan
         eax = rdi + 1
         rbx = eax
         rbx += rbx               // reloc.OLEAUT32.dll_SysReAllocString
         rcx = rbx                // reloc.OLEAUT32.dll_SysReAllocString
         fcn.14004bee0 ()
         r13 = rax
         qword [rbp - 0x79] = rax
         r8 = rbx                 // reloc.OLEAUT32.dll_SysReAllocString
         rdx = rbp - 0x19
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x177fe7)

    loc_0x140017bd5: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017bac(x)
         ecx = 8                  // reloc.OLEAUT32.dll_VariantClear
         fcn.14004bee0 ()
         rbx = rax
         xmm0 = qword [0x14006d778] // [0x14006d778:8]=0x76002d0020
         qword [rax] = xmm0
         r12 = rip + 0x533c3      // 0x14006afb8
         v = r13 & r13
         if (!zf) r12 = r13
         v = edi & edi
         if (v <= 0) 
         goto loc_0x140017c00
    loc_0x140017c00: // orphan
         r15d = rdi + 3
         eax = r15 + 1
         v = eax - 4
         if (v <= 0) 
         goto loc_0x140017c0d
    loc_0x140017c0d: // orphan
         v = eax - 8              // reloc.OLEAUT32.dll_VariantClear // "4"
         ecx = 8                  // reloc.OLEAUT32.dll_VariantClear
         cmovle eax ecx           // reloc.OLEAUT32.dll_VariantClear
         v = eax - 0x10
         ecx = 0x10
         if (v < 0) eax = ecx
         rdx = eax
         rdx += rdx
         rcx = rbx
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) rbx = rax

    loc_0x140017c38: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017c0b(x)
         r8 = edi
         r8 += r8
         rcx = rbx + 6            // reloc.OLEAUT32.dll_SysStringLen
         rdx = r12
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rax = r15d
         r12d = 0
         word [rbx + rax*2] = r12w
         
         goto loc_0x140017c57
    loc_0x140017c57: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017bfe(x)
         r12d = 0

    loc_0x140017c5a: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017c55(x)
         rdi = rip + 0x53357      // 0x14006afb8
         v = rbx & rbx
         if (!zf) rdi = rbx
         v = r15d & r15d
         if (v <= 0) 
         goto loc_0x140017c6d
    loc_0x140017c6d: // orphan
         r12 = dword [rbp - 0x59]
         r13d = r12 + r15
         ecx = r13 + 1            // reloc.OLEAUT32.dll_SysStringLen
         v = ecx - r14d
         if (v <= 0) 
         goto loc_0x140017c7e
    loc_0x140017c7e: // orphan
         eax = r14 + r14
         v = eax - ecx
         cmovge ecx eax
         v = ecx - 0x10
         eax = 0x10
         if (v < 0) ecx = eax
         rdx = ecx
         rdx += rdx
         rcx = rsi
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) rsi = rax

    loc_0x140017ca7: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017c7c(x)
         r8 = r15d
         r8 += r8
         rcx = rsi + r12*2
         rdx = rdi
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rax = r13d
         r12d = 0
         word [rsi + rax*2] = r12w
         r13 = qword [rbp - 0x79]

    loc_0x140017cc8: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017c6b(x)
         rcx = rbx
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = r13
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = qword [rbp - 0x29]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rax = rip + 0x532cf      // 0x14006afb8
         r9 = rax
         v = rsi & rsi
         if (!zf) r9 = rsi        // LPCWSTR lpParameters
         rbx = qword [rbp - 0x69]
         v = rbx & rbx
         if (!zf) rax = rbx
         dword [nShowCmd] = 1     // INT nShowCmd
         qword [lpDirectory] = r12 // LPCWSTR lpDirectory
         r8 = rax                 // LPCWSTR lpFile
         edx = 0                  // LPCWSTR lpOperation
         ecx = 0                  // HWND hwnd
         qword [sym.imp.SHELL32.dll_ShellExecuteW] () // [0x14006a5f8:8]=0x927d0 reloc.SHELL32.dll_ShellExecuteW // (pstr 0xffffffff) "\n" reloc.SHELL32.dll_ShellExecuteW // HINSTANCE ShellExecuteW(0, 0, "", "", -1, -1)
         rcx = qword [rbp - 0x41]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = rsi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)

    loc_0x140017d2c: // orphan
         // CODE XREF from fcn.1400178e0 @ 0x140017a1d(x)
         rcx = qword [rbp - 0x51]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = rbx
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         eax = 0
         rcx = qword [rbp + 0x27]
         rcx ^= rsp
         fcn.140047380 ()         // fcn.140047380(0x0)
         r11 = var_e0h
         rbx = qword [r11 + 0x38]
         rsi = qword [r11 + 0x40]
         rdi = qword [r11 + 0x48]
         rsp = r11
         r15 = pop ()             // (pstr 0xffffffff) "\n"
         r14 = pop ()
         r13 = pop ()
         r12 = pop ()
         rbp = pop ()
         return

        return rax;
}

```

## keyboard_hook_install

```c
===== FUNCTION keyboard_hook_install 0x1400340b0 =====
[{"addr":5368922288,"name":"fcn.1400340b0","size":66,"is-pure":"false","realsz":66,"noreturn":false,"recursive":false,"stackframe":80,"calltype":"ms","cost":18,"cc":1,"bits":64,"type":"fcn","nbbs":1,"tracecov":0,"is-lineal":true,"ninstrs":12,"edges":0,"ebbs":1,"signature":"fcn.1400340b0 ();","minaddr":5368922288,"maxaddr":5368922354,"maxbbins":12,"midbbins":12.000,"ratbbins":1.000,"indegree":3,"outdegree":2,"nlocals":0,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.1400340b0 (int rcx, int rdx) {
    loc_0x1400340b0:
        // CALL XREFS from fcn.140031d70 @ 0x140031f71(x), 0x140032200(x)
        // CODE XREF from fcn.1400325e0 @ 0x1400325e7(x)
        // NULL XREF from fcn.140034050 @ +0x5a(x)
        rsp -= 0x28
        rcx = rip + 0x68855 // 0x14009c910
        fcn.140035760 () // fcn.140035760(0x0)
        r8 = qword [0x14009c880] // [0x14009c880:8]=0 // HINSTANCE hmod
        rdx = rip - fcn.140031d70 // 0x140031d70 // "@UVWATAUAVAWH\x83\xec`H\xc7D$H\xfe" // HOOKPROC lpfn
        r9d = 0      // DWORD dwThreadId
        ecx = r9 + 0xd
        qword [sym.imp.USER32.dll_SetWindowsHookExW] () // [0x14006a7a0:8]=0x924f4 reloc.USER32.dll_SetWindowsHookExW // (pstr 0xffffffff) "\n" reloc.USER32.dll_SetWindowsHookExW // HHOOK SetWindowsHookExW(-1, 0x140031d70, 0, 0)
        rcx = rip + 0x6882e // 0x14009c910
        qword [0x14009c888] = rax // [0x14009c888:8]=0
        rsp += 0x28
        goto loc_0x140035790 // fcn.140035790 // fcn.140035790 // fcn.140035790(0x0)
        return rax;
}

```

## main_window_init_controller

```c
===== FUNCTION main_window_init_controller 0x140034f90 =====
[{"addr":5368926096,"name":"fcn.140034f90","size":1683,"is-pure":"false","realsz":1683,"noreturn":false,"recursive":false,"stackframe":848,"calltype":"ms","cost":607,"cc":36,"bits":64,"type":"fcn","nbbs":57,"tracecov":0,"is-lineal":true,"ninstrs":380,"edges":91,"ebbs":1,"signature":"fcn.140034f90 (int64_t arg1, int64_t arg2);","minaddr":5368926096,"maxaddr":5368927779,"maxbbins":71,"midbbins":6.667,"ratbbins":10.650,"indegree":1,"outdegree":76,"nlocals":26,"nargs":2,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.140034f90 (int64_t arg1, int64_t arg2) {
    loc_0x140034f90:
        // CALL XREF from entry0 @ 0x1400480cd(x)
        push (rbp)
        push (rbx)
        push (rdi)
        push (r14)
        push (r15)
        rbp = rsp - 0x200
        rsp -= 0x300
        rax = qword [0x14009a9e8] // [0x14009a9e8:8]=0x2b992ddfa200
        rax ^= rsp
        qword [var_1f0h] = rax
        rbx = r8     // arg3
        rdi = rcx    // arg1
        fcn.140037fe0 ()
        r14d = 1
        rcx = rax
        edx = r14d
        fcn.140037e50 () // fcn.140037e50(0x0, 0x0)
        fcn.140037fe0 ()
        rcx = rax
        fcn.140038070 () // fcn.140038070(0x0)
        fcn.140037fe0 ()
        rcx = rax
        fcn.140037ec0 ()
        rcx = rbx    // uint32_t arg1
        fcn.1400343f0 () // fcn.1400343f0(0x0)
        rbx = rip + 0x67983 // 0x14009c980
        rcx = rbx
        fcn.1400366b0 () // fcn.1400366b0(0x0)
        v = byte [0x14009c89a] - 0 // [0x14009c89a:1]=0
        if (!v) goto loc_0x140035034 // likely
        goto loc_0x14003500e;
    loc_0x140035034:
        // CODE XREFS from fcn.140034f90 @ 0x14003500c(x), 0x140035015(x), 0x140035023(x)
        v = byte [0x14009c8e6] - 0 // [0x14009c8e6:1]=0
        if (!v) goto loc_0x14003504b // likely
        goto loc_0x14003503d;
    loc_0x14003504b:
        // CODE XREF from fcn.140034f90 @ 0x14003503b(x)
        qword [var_338h] = rsi
        fcn.140016d90 ()
        ecx = byte [0x14009c9b9] // [0x14009c9b9:1]=0
        r15d = 0
        v = eax & eax
        if (!v) ecx = r15d
        byte [0x14009c9b9] = cl // [0x14009c9b9:1]=0
        fcn.1400177a0 ()
        fcn.140016d90 ()
        v = eax & eax
        if (!v) goto loc_0x140035168 // likely
        goto loc_0x140035080;
    loc_0x140035168:
        // CODE XREFS from fcn.140034f90 @ 0x14003507a(x), 0x140035158(x)
        fcn.140033070 ()
        rcx = rbx
        fcn.140036850 () // fcn.140036850(0x0)
        v = eax & eax
        if (v) goto loc_0x1400351a7 // unlikely
        goto loc_0x140035179;
    loc_0x1400351a7:
        // CODE XREF from fcn.140034f90 @ 0x140035177(x)
        fcn.1400160a0 ()
        r9d = 0x20   // int cchBufferMax
        qword [0x14009c880] = rdi // [0x14009c880:8]=0
        r8 = rip + 0x676e0 // 0x14009c8a0 // LPWSTR lpBuffer
        rcx = rdi    // HINSTANCE hInstance
        edx = r9 + 0x4e
        qword [sym.imp.USER32.dll_LoadStringW] () // [0x14006a790:8]=0x9251e reloc.USER32.dll_LoadStringW // (pstr 0xffffffff) "\n" reloc.USER32.dll_LoadStringW // int LoadStringW(0, 0x00000000, 0x0000000000000000, 0x00000006)
        rax = rip - 0x4d54 // 0x140030480
        dword [ARG_0] = 0x50 // 'P' // [0x50:4]=0
        qword [var_78h] = rax
        qword [var_80h] = r15
        qword [var_bp_78h] = rdi
        qword [var_68h] = r15
        fcn.140015e80 ()
        qword [var_bp_70h] = rax
        fcn.140017810 ()
        ecx = 0
        qword [var_48h] = rax
        qword [sym.imp.GDI32.dll_GetStockObject] () // [0x14006a1b0:8]=0x926be reloc.GDI32.dll_GetStockObject // (pstr 0xffffffff) "\n" reloc.GDI32.dll_GetStockObject // hgdiobj GetStockObject(void)
        rcx = ARG_0  // const WNDCLASSEXW *ARG_0
        dword [var_74h] = r15d
        qword [var_60h] = rax
        rax = rip + 0x409bc // u"EVKey_MainWnd" // 0x140075bd8
        qword [var_50h] = rax
        rax = rip + str.EV_Vietnamese_Keyboard // 0x140075bf8 // "E\rV"
        qword [var_58h] = rax
        qword [sym.imp.USER32.dll_RegisterClassExW] () // [0x14006a7c0:8]=0x924a2 reloc.USER32.dll_RegisterClassExW // (pstr 0xffffffff) "\n" reloc.USER32.dll_RegisterClassExW // ATOM RegisterClassExW(0x178050)
        v = ax & ax
        if (!v) goto loc_0x14003510a // unlikely
        goto loc_0x14003523a;
    loc_0x14003510a:
        // CODE XREFS from fcn.140034f90 @ 0x1400350f3(x), 0x140035234(x), 0x140035296(x), 0x1400353e8(x)
        eax = 0
        
    loc_0x14003510c:
        // CODE XREF from fcn.140034f90 @ 0x14003561e(x)
        rsi = qword [var_338h]
        rcx = qword [var_1f0h]
        rcx ^= rsp
        fcn.140047380 () // fcn.140047380(0x0)
        rsp += 0x300
        r15 = pop ()
        r14 = pop ()
        rdi = pop ()
        rbx = pop ()
        rbp = pop ()
        return
        goto loc_0x14003529c;
        goto loc_0x140035092;
        goto loc_0x140035017;
        return rax;
    loc_0x140035017:
        fcn.140035df0 ()
        v = byte [0x14009c9d3] - 0 // [0x14009c9d3:1]=0
        if (v) goto loc_0x140035034 // unlikely
        goto loc_0x140035025;
    loc_0x140035025:
        rcx = rbx
        byte [0x14009c9d3] = r14b // [0x14009c9d3:1]=0
        fcn.140036850 () // fcn.140036850(0x0)
    loc_0x14003503d: // orphan
         byte [0x14009c9b9] = r14b // [0x14009c9b9:1]=0
         byte [0x14009c9e8] = r14b // [0x14009c9e8:1]=0

    loc_0x140035080: // orphan
         rcx = rip + 0x40bd1      // u"user32.dll" // 0x140075c58 // LPCWSTR lpLibFileName
         qword [sym.imp.KERNEL32.dll_LoadLibraryW] () // [0x14006a1c8:8]=0x919d4 reloc.KERNEL32.dll_LoadLibraryW // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_LoadLibraryW // HMODULE LoadLibraryW("u")
         v = rax & rax
         if (!v) 
         goto loc_0x140035092
    loc_0x140035092: // orphan
         rdx = rip + str.ChangeWindowMessageFilter // 0x140075c70 // "ChangeWindowMessageFilter" // LPCSTR lpProcName
         rcx = rax                // HMODULE hModule
         qword [sym.imp.KERNEL32.dll_GetProcAddress] () // [0x14006a4c8:8]=0x918e2 reloc.KERNEL32.dll_GetProcAddress // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GetProcAddress // FARPROC GetProcAddress(0, 0x695765676e616843)
         qword [0x14009c8f8] = rax // [0x14009c8f8:8]=50
         
         goto loc_0x1400350ab
    loc_0x1400350ab: // orphan
         // CODE XREF from fcn.140034f90 @ 0x140035090(x)
         rax = qword [0x14009c8f8] // [0x14009c8f8:8]=0

    loc_0x1400350b2: // orphan
         // CODE XREF from fcn.140034f90 @ 0x1400350a9(x)
         v = rax & rax
         if (!v) 
         goto loc_0x1400350b7
    loc_0x1400350b7: // orphan
         edx = r14d
         ecx = 0x402
         rax ()                   // 0x1400350c1(0x0, 0x0, 0x0, 0x402)

    loc_0x1400350c1: // orphan
         // CODE XREF from fcn.140034f90 @ 0x1400350b5(x)
         v = byte [0x14009c9b9] - r15b // [0x14009c9b9:1]=0
         if (!v) 
         goto loc_0x1400350ca
    loc_0x1400350ca: // orphan
         fcn.140016b70 ()
         v = al & al
         if (v) 
    loc_0x1400350d3: // orphan
         v = byte [0x14009c8e7] - r15b // [0x14009c8e7:1]=0
         if (v) 
    loc_0x1400350dc: // orphan
         r8d = byte [0x14009c89b] // [0x14009c89b:1]=0
         r9d = 0
         edx = 0
         ecx = r14d
         fcn.1400175f0 ()         // fcn.1400175f0(0x0, 0x0, 0x0, 0x0)
         v = al & al
         if (v) 
         goto loc_0x1400350f5
    loc_0x1400350f5: // orphan
         r8d = byte [0x14009c89b] // [0x14009c89b:1]=0
         r9d = 0
         edx = r14d

    loc_0x140035103: // orphan
         // CODE XREF from fcn.140034f90 @ 0x14003514f(x)
         ecx = 0

    loc_0x140035105: // orphan
         // CODE XREF from fcn.140034f90 @ 0x140035166(x)
         fcn.1400175f0 ()         // fcn.1400175f0(0x0, 0x0, 0x0, 0x0)

    loc_0x14003510c: // orphan
         // CODE XREF from fcn.140034f90 @ 0x14003561e(x)
         rsi = qword [var_338h]
         rcx = qword [var_1f0h]
         rcx ^= rsp
         fcn.140047380 ()         // fcn.140047380(0x0)
         rsp += 0x300
         r15 = pop ()
         r14 = pop ()
         rdi = pop ()
         rbx = pop ()
         rbp = pop ()
         return

        return rax;
    loc_0x140035132: // orphan
         // CODE XREFS from fcn.140034f90 @ 0x1400350c8(x), 0x1400350d1(x), 0x1400350da(x)
         eax = byte [0x14009c89b] // [0x14009c89b:1]=0
         v = al & al
         if (!v) 
         goto loc_0x14003513d
    loc_0x14003513d: // orphan
         v = byte [0x14009c8e4] - r15b // [0x14009c8e4:1]=0
         if (v) 
    loc_0x140035146: // orphan
         r9d = 0
         r8d = al
         edx = 0
         
         goto loc_0x140035151
    loc_0x140035151: // orphan
         // CODE XREFS from fcn.140034f90 @ 0x14003513b(x), 0x140035144(x)
         v = byte [0x14009c8e6] - r15b // [0x14009c8e6:1]=0
         if (!v) 
         goto loc_0x14003515a
    loc_0x14003515a: // orphan
         r9d = 0
         r8d = al
         edx = 0
         ecx = r14d
         
    loc_0x140035179: // orphan
         v = byte [0x14009c9bc] - r15b // [0x14009c9bc:1]=0
         rax = rip + 0x40759      // u"Kh\xf4ng th\u1ec3 l\u01b0u c\u1ea5u h\xecnh, c\xf3 th\u1ec3 EVKey \u0111ang \u1edf v\u1ecb tr\xed kh\xf4ng c\xf3 quy" // 0x1400758e0
         rdx = rip + str.Cannot_save_settings__please_restart_EVKey_as_Administrator_or_move_EVKey_to_another_directory_location // 0x140075a30 // u"Cannot save settings, please restart EVKey as Administrator or move EVKey to another directory location" // LPCWSTR lpText
         r9d = 0x30               // '0' // UINT uType
         if (!zf) rdx = rax
         r8 = rip + 0x40961       // u"EVKey" // 0x140075b00 // LPCWSTR lpCaption
         ecx = 0                  // HWND hWnd
         qword [sym.imp.USER32.dll_MessageBoxW] () // [0x14006aa48:8]=0x91f90 reloc.USER32.dll_MessageBoxW // (pstr 0xffffffff) "\n" reloc.USER32.dll_MessageBoxW // int MessageBoxW(0, "C", "E", 0x00000000)

    loc_0x14003523a: // orphan
         qword [var_sp_58h] = rdi
         r9 = rip + 0x6765a       // 0x14009c8a0
         qword [var_sp_50h] = r15
         r8 = rip + 0x408be       // u"EVKey_MainWnd" // 0x140075b10
         qword [var_sp_48h] = r15
         rcx = rip + str._ba_a_   // 0x140096140
         dword [var_40h] = 0x80000000 // [0x80000000:4]=0xff000000
         edx = 0x80
         dword [var_38h] = 0x80000000 // [0x80000000:4]=0xff000000
         dword [var_30h] = 0x80000000 // [0x80000000:4]=0xff000000
         dword [lpThreadId] = 0x80000000 // [0x80000000:4]=0xff000000
         dword [dwCreationFlags] = 0xcf0000 // [0xcf0000:4]=-1
         fcn.140030080 ()         // fcn.140030080(0x0, 0x0, 0x80, 0x140096140, 0x140075b10, 0x14009c8a0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0)
         rsi = rax
         v = rax & rax
         if (!v) 
    loc_0x14003529c: // orphan
         edx = 0                  // int nCmdShow
         rcx = rax                // HWND hWnd
         qword [sym.imp.USER32.dll_ShowWindow] () // [0x14006a998:8]=0x920f8 reloc.USER32.dll_ShowWindow // (pstr 0xffffffff) "\n" reloc.USER32.dll_ShowWindow // BOOL ShowWindow(0, 0)
         rcx = rsi                // HWND hWnd
         qword [sym.imp.USER32.dll_UpdateWindow] () // [0x14006a928:8]=0x9214c reloc.USER32.dll_UpdateWindow // "L!\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_UpdateWindow // BOOL UpdateWindow(0)
         fcn.140015e80 ()
         r9 = rax                 // LPARAM lParam
         r8 = r14                 // WPARAM wParam
         edx = 0x80               // UINT Msg
         rcx = rsi                // HWND hWnd
         qword [sym.imp.USER32.dll_SendMessageW] () // [0x14006a968:8]=0x91f2a reloc.USER32.dll_SendMessageW // "*\x1f\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_SendMessageW // LRESULT SendMessageW(0, 0x00000000, 0, 0)
         fcn.140017810 ()
         r9 = rax                 // LPARAM lParam
         r8d = 0                  // WPARAM wParam
         edx = 0x80               // UINT Msg
         rcx = rsi                // HWND hWnd
         qword [sym.imp.USER32.dll_SendMessageW] () // [0x14006a968:8]=0x91f2a reloc.USER32.dll_SendMessageW // "*\x1f\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_SendMessageW // LRESULT SendMessageW(0, 0x00000000, 0, 0)
         rcx = rip + 0x67637      // 0x14009c920
         qword [0x14009c958] = rbx // [0x14009c958:8]=0
         qword [0x14009c960] = rdi // [0x14009c960:8]=0x140075bf100
         qword [0x14009c968] = rsi // [0x14009c968:8]=0
         fcn.14002fa90 ()         // fcn.14002fa90(0x0)
         rcx = pRawInputDevices
         word [0x14009c980] = r15w // [0x14009c980:2]=0
         fcn.1400163e0 ()         // fcn.1400163e0(0x0)
         rcx = rax
         rbx = rax
         edx = dword [rax + 8]
         edx += 0xe
         fcn.140008a10 ()         // fcn.140008a10(0x0, 0x0)
         rdx = dword [rbx + 8]
         rcx = qword [rbx]
         xmm0 = xmmword [str.evkmacro.txt] // [0x140075b30:16]=-1 // u"\evkmacro.txt"
         r8 = rcx + rdx*2
         rdx = rbx
         xmmword [r8] = xmm0
         xmm1 = qword [0x140075b40] // [0x140075b40:8]=0x780074002e006f
         qword [r8 + 0x10] = xmm1
         ecx = dword [0x140075b48] // [0x140075b48:4]=116 // u"cr"
         dword [r8 + 0x18] = ecx
         rcx = rip + 0x60e0a      // 0x140096168
         dword [rbx + 8] += 0xd   // [0xd:4]=0x65007300
         fcn.14000adb0 ()         // fcn.14000adb0(0x0, 0x0)
         rcx = qword [pRawInputDevices]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         r9d = 0                  // LPVOID lpParameter
         qword [lpThreadId] = r15 // LPDWORD lpThreadId
         r8 = rip - fcn.140034700 // 0x140034700 // "H\x83\xec(\xb0\x01\x88\x05\x8c\x81\x06" // LPTHREAD_START_ROUTINE lpStartAddress
         dword [dwCreationFlags] = r15d // DWORD dwCreationFlags
         edx = 0                  // SIZE_T dwStackSize
         ecx = 0                  // LPSECURITY_ATTRIBUTES lpThreadAttributes
         qword [sym.imp.KERNEL32.dll_CreateThread] () // [0x14006a528:8]=0x9180c reloc.KERNEL32.dll_CreateThread // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CreateThread // HANDLE CreateThread(0, 0, 0x058801b028ec8348, 0, -1, -1)
         r9d = 0                  // LPCWSTR lpName
         r8d = 0                  // BOOL bInitialState
         edx = 0                  // BOOL bManualReset
         qword [0x140096128] = rax // [0x140096128:8]=0
         ecx = 0                  // LPSECURITY_ATTRIBUTES lpEventAttributes
         qword [sym.imp.KERNEL32.dll_CreateEventW] () // [0x14006a500:8]=0x9185c reloc.KERNEL32.dll_CreateEventW // "\\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CreateEventW // HANDLE CreateEventW(0, 0, 0, 0)
         rcx = rip + str.SHELLHOOK // 0x140075b50 // u"SHELLHOOK" // LPCWSTR lpString
         qword [0x14009c918] = rax // [0x14009c918:8]=0
         qword [sym.imp.USER32.dll_RegisterWindowMessageW] () // [0x14006a7e8:8]=0x92440 reloc.USER32.dll_RegisterWindowMessageW // "@$\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_RegisterWindowMessageW // UINT RegisterWindowMessageW("S")
         rcx = rsi                // HWND hwnd
         dword [0x140096188] = eax // [0x140096188:4]=0
         qword [sym.imp.USER32.dll_RegisterShellHookWindow] () // [0x14006a7a8:8]=0x924da reloc.USER32.dll_RegisterShellHookWindow // (pstr 0xffffffff) "\n" reloc.USER32.dll_RegisterShellHookWindow // BOOL RegisterShellHookWindow(0)
         rcx = qword [0x140096148] // [0x140096148:8]=0
         rdx = rip + 0x67519      // 0x14009c8f0
         fcn.140030740 ()         // fcn.140030740(0x0, 0x0)
         fcn.140034760 ()
         fcn.1400325f0 ()
         v = eax & eax
         if (!v) 
    loc_0x1400353ee: // orphan
         ecx = 0
         fcn.140016ec0 ()
         v = al & al
         if (v) 
         goto loc_0x1400353f9
    loc_0x1400353f9: // orphan
         fcn.1400325d0 ()

    loc_0x140035400: // orphan
         // CODE XREF from fcn.140034f90 @ 0x140035413(x)
         ecx = 0x64               // 'd' // DWORD dwMilliseconds
         sub.KERNEL32.dll_Sleep () // VOID Sleep(0x00000000)
         ecx = 0
         fcn.140016ec0 ()
         v = al & al
         if (!v) 
         goto loc_0x140035415
    loc_0x140035415: // orphan
         fcn.1400325e0 ()

    loc_0x14003541a: // orphan
         // CODE XREF from fcn.140034f90 @ 0x1400353f7(x)
         v = byte [0x14009c982] - r15b // [0x14009c982:1]=0
         if (v) 
         goto loc_0x140035423
    loc_0x140035423: // orphan
         v = byte [0x14009c8e5] - r15b // [0x14009c8e5:1]=0
         if (v) 
    loc_0x14003542c: // orphan
         ecx = 0
         fcn.1400178a0 ()
         
         goto loc_0x140035435
    loc_0x140035435: // orphan
         // CODE XREFS from fcn.140034f90 @ 0x140035421(x), 0x14003542a(x)
         rcx = qword [0x140096148] // [0x140096148:8]=0 // HWND hWnd
         r9d = 0                  // LPARAM lParam
         edx = 0x111              // UINT Msg
         r8d = 0xc352             // WPARAM wParam // (pstr 0xffffffff) "\n"
         qword [sym.imp.USER32.dll_PostMessageW] () // [0x14006a8a0:8]=0x91d94 reloc.USER32.dll_PostMessageW // (pstr 0xffffffff) "\n" reloc.USER32.dll_PostMessageW // BOOL PostMessageW(0, 0x00000000, 0xc352, 0)

    loc_0x140035450: // orphan
         // CODE XREF from fcn.140034f90 @ 0x140035433(x)
         v = byte [0x14009c9d6] - r15b // [0x14009c9d6:1]=0
         if (!v) 
         goto loc_0x140035459
    loc_0x140035459: // orphan
         edx = byte [0x14009c89b] // [0x14009c89b:1]=0
         ecx = r14b
         fcn.140017480 ()

    loc_0x140035469: // orphan
         // CODE XREF from fcn.140034f90 @ 0x140035457(x)
         r9d = 0                  // UINT wMsgFilterMax
         rcx = lpMsg              // LPMSG lpMsg
         r8d = 0                  // UINT wMsgFilterMin
         edx = 0                  // HWND hWnd
         qword [sym.imp.USER32.dll_GetMessageW] () // [0x14006a7e0:8]=0x9245a reloc.USER32.dll_GetMessageW // "Z$\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetMessageW // BOOL GetMessageW(0x00000001400353dc, 0, 0, 0)
         v = eax & eax
         if (!v) 
         goto loc_0x140035483
    loc_0x140035483: // orphan
         // CODE XREF from fcn.140034f90 @ 0x140035549(x)
         rdx = qword [0x140096180] // [0x140096180:8]=0x40075bf10000
         v = rdx & rdx
         if (!v) 
         goto loc_0x14003548f
    loc_0x14003548f: // orphan
         rdx = qword [rdx]
         v = rdx & rdx
         if (!v) 
    loc_0x140035497: // orphan
         rax = qword [rdx + 8]
         rcx = dword [rax + 4]
         rcx = qword [rcx + rdx + 0x10] // HWND hWnd // (pstr 0xffffffff) "\n"
         qword [sym.imp.USER32.dll_IsWindow] () // [0x14006a970:8]=0x91f3a reloc.USER32.dll_IsWindow // ":\x1f\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_IsWindow // BOOL IsWindow(-1)
         v = eax & eax
         if (!v) 
    loc_0x1400354ae: // orphan
         rax = qword [0x140096180] // [0x140096180:8]=0x40075bf10000
         rdx = lpMsg              // LPMSG lpMsg
         r8 = qword [rax]
         rax = qword [r8 + 8]
         rcx = dword [rax + 4]
         rcx = qword [rcx + r8 + 0x10] // HWND hDlg // (pstr 0xffffffff) "\n"
         qword [sym.imp.USER32.dll_IsDialogMessageW] () // [0x14006a788:8]=0x9252c reloc.USER32.dll_IsDialogMessageW // ",%\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_IsDialogMessageW // BOOL IsDialogMessageW(-1, 0x00000001400353dc)
         v = eax & eax
         if (v) 
         goto loc_0x1400354d3
    loc_0x1400354d3: // orphan
         // CODE XREFS from fcn.140034f90 @ 0x14003548d(x), 0x140035495(x), 0x1400354ac(x)
         rdx = qword [0x140096178] // [0x140096178:8]=0
         v = rdx & rdx
         if (!v) 
         goto loc_0x1400354df
    loc_0x1400354df: // orphan
         rdx = qword [rdx]
         v = rdx & rdx
         if (!v) 
    loc_0x1400354e7: // orphan
         rax = qword [rdx]
         rcx = dword [rax + 4]
         rcx = qword [rcx + rdx + 8] // HWND hWnd
         qword [sym.imp.USER32.dll_IsWindow] () // [0x14006a970:8]=0x91f3a reloc.USER32.dll_IsWindow // ":\x1f\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_IsWindow // BOOL IsWindow(-1)
         v = eax & eax
         if (!v) 
    loc_0x1400354fd: // orphan
         rax = qword [0x140096178] // [0x140096178:8]=0
         rdx = lpMsg              // LPMSG lpMsg
         r8 = qword [rax]
         rax = qword [r8]
         rcx = dword [rax + 4]
         rcx = qword [rcx + r8 + 8] // HWND hDlg
         qword [sym.imp.USER32.dll_IsDialogMessageW] () // [0x14006a788:8]=0x9252c reloc.USER32.dll_IsDialogMessageW // ",%\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_IsDialogMessageW // BOOL IsDialogMessageW(-1, 0x00000001400353dc)
         v = eax & eax
         if (v) 
    loc_0x140035521: // orphan
         // CODE XREFS from fcn.140034f90 @ 0x1400354dd(x), 0x1400354e5(x), 0x1400354fb(x)
         rcx = lpMsg              // const MSG *lpMsg
         qword [sym.imp.USER32.dll_TranslateMessage] () // [0x14006a7d8:8]=0x92468 reloc.USER32.dll_TranslateMessage // "h$\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_TranslateMessage // BOOL TranslateMessage(0x177fc0)
         rcx = lpMsg              // const MSG *lpMsg
         qword [sym.imp.USER32.dll_DispatchMessageW] () // [0x14006a7d0:8]=0x9247c reloc.USER32.dll_DispatchMessageW // "|$\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_DispatchMessageW // LRESULT DispatchMessageW(0x177fc0)

    loc_0x140035535: // orphan
         // CODE XREFS from fcn.140034f90 @ 0x1400354d1(x), 0x14003551f(x)
         r9d = 0                  // UINT wMsgFilterMax
         rcx = lpMsg              // LPMSG lpMsg
         r8d = 0                  // UINT wMsgFilterMin
         edx = 0                  // HWND hWnd
         qword [sym.imp.USER32.dll_GetMessageW] () // [0x14006a7e0:8]=0x9245a reloc.USER32.dll_GetMessageW // "Z$\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetMessageW // BOOL GetMessageW(0x00000001400353dc, 0, 0, 0)
         v = eax & eax
         if (v) 
         goto loc_0x14003554f
    loc_0x14003554f: // orphan
         // CODE XREF from fcn.140034f90 @ 0x14003547d(x)
         fcn.1400376d0 ()
         fcn.140031c50 ()
         rcx = rip + str.input.dll // 0x140075e98 // u"input.dll" // LPCWSTR lpLibFileName
         qword [sym.imp.KERNEL32.dll_LoadLibraryW] () // [0x14006a1c8:8]=0x919d4 reloc.KERNEL32.dll_LoadLibraryW // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_LoadLibraryW // HMODULE LoadLibraryW("i")
         rbx = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14003556e
    loc_0x14003556e: // orphan
         rdx = rip + str.InstallLayoutOrTip // 0x140075eb0 // "InstallLayoutOrTip" // LPCSTR lpProcName
         rcx = rax                // HMODULE hModule
         qword [sym.imp.KERNEL32.dll_GetProcAddress] () // [0x14006a4c8:8]=0x918e2 reloc.KERNEL32.dll_GetProcAddress // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GetProcAddress // FARPROC GetProcAddress(0, 0x4c6c6c6174736e49)
         rdi = rax
         v = rax & rax
         if (!v) 
         goto loc_0x140035586
    loc_0x140035586: // orphan
         edx = 0
         rcx = var_10h
         r8d = 0x200
         fcn.140049090 ()         // fcn.140049090(0x0, 0x0)
         r8d = 0x409              //  ... // (pstr 0xffffffff) "\n"
         rdx = rip + str._04X:0BAD015B_A9AA_41E4_B817_803DD9E81A2AF74F81AF_6F78_42C7_B006_2630C950B9A2 // 0x140075dc0 // u"%04X:{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}{F74F81AF-6F78-42C7-B006-2630C950B9A2}" // LPCWSTR ARG_1
         rcx = var_10h            // LPWSTR ARG_0
         qword [sym.imp.USER32.dll_wsprintfW] () // [0x14006a908:8]=0x91e76 reloc.USER32.dll_wsprintfW // "v\x1e\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_wsprintfW // int wsprintfW(0x00000001400355ae, "%", 0x409)
         edx = r14d
         rcx = var_10h
         rdi ()                   // 0x1400355b7(0x0, 0x0, 0x0, 0x177ff0)

    loc_0x1400355b7: // orphan
         // CODE XREF from fcn.140034f90 @ 0x140035584(x)
         rcx = rbx                // HMODULE hLibModule
         qword [sym.imp.KERNEL32.dll_FreeLibrary] () // [0x14006a248:8]=0x91b06 reloc.KERNEL32.dll_FreeLibrary // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_FreeLibrary // BOOL FreeLibrary(0)

    loc_0x1400355c0: // orphan
         // CODE XREF from fcn.140034f90 @ 0x14003556c(x)
         fcn.1400357a0 ()
         fcn.140040610 ()
         r8d = 0x10               // UINT cbSize
         dword [pRawInputDevices] = 0x20001 // [0x20001:4]=-1
         edx = r14d               // UINT uiNumDevices
         dword [var_64h] = r14d
         rcx = pRawInputDevices   // PCRAWINPUTDEVICE pRawInputDevices
         qword [var_sp_68h] = r15
         qword [sym.imp.USER32.dll_RegisterRawInputDevices] () // [0x14006a678:8]=0x92552 reloc.USER32.dll_RegisterRawInputDevices // "R%\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_RegisterRawInputDevices // BOOL RegisterRawInputDevices(0x0000000000020001, 0, 0x00000065)
         r8d = 0x10               // UINT cbSize
         dword [pRawInputDevices] = 0x60001 // [0x60001:4]=-1
         edx = r14d               // UINT uiNumDevices
         dword [var_64h] = r14d
         rcx = pRawInputDevices   // PCRAWINPUTDEVICE pRawInputDevices
         qword [var_sp_68h] = r15
         qword [sym.imp.USER32.dll_RegisterRawInputDevices] () // [0x14006a678:8]=0x92552 reloc.USER32.dll_RegisterRawInputDevices // "R%\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_RegisterRawInputDevices // BOOL RegisterRawInputDevices(0x0000000000060001, 0, 0x00000065)
         fcn.1400325d0 ()
         eax = r14d
         
}

```

## single_instance_mutex

```c
===== FUNCTION single_instance_mutex 0x140035630 =====
[{"addr":5368927792,"name":"fcn.140035630","size":175,"is-pure":"false","realsz":175,"noreturn":false,"recursive":false,"stackframe":96,"calltype":"ms","cost":72,"cc":6,"bits":64,"type":"fcn","nbbs":8,"tracecov":0,"is-lineal":true,"ninstrs":51,"edges":8,"ebbs":3,"signature":"fcn.140035630 (int64_t arg1);","minaddr":5368927792,"maxaddr":5368927967,"maxbbins":13,"midbbins":6.375,"ratbbins":2.039,"indegree":4,"outdegree":5,"nlocals":3,"nargs":1,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.140035630 (int64_t arg1) {
    loc_0x140035630:
        // CALL XREFS from section..text @ +0x6b(x), +0xecb(x), +0x1fbb(x)
        // CALL XREF from fcn.140003e80 @ 0x140003e97(x)
        push (rdi)
        rsp -= 0x30
        qword [var_20h] = 0xfffffffffffffffe
        qword [var_48h] = rbx
        rdi = rcx    // arg1
        fcn.140016d90 ()
        v = eax & eax
        if (!v) goto loc_0x140035689 // likely
        goto loc_0x140035650;
    loc_0x140035689:
        // CODE XREF from fcn.140035630 @ 0x14003564e(x)
        ecx = 0x10   // int64_t arg1
        fcn.140047b24 () // fcn.140047b24(0x0)
        rbx = rax
        qword [var_40h] = rax
        v = rax & rax
        if (!v) goto loc_0x1400356cc // likely
        goto loc_0x1400356a0;
    loc_0x1400356cc:
        // CODE XREF from fcn.140035630 @ 0x14003569e(x)
        ebx = 0
        rax = rdi
        qword [rdi] = rbx
        rbx = qword [var_48h]
        rsp += 0x30
        rdi = pop ()
        return
        goto loc_0x140035664;
        return rax;
    loc_0x140035664:
        edx = 0xfa0  // 4000 // int64_t arg2 // (pstr 0xffffffff) "\n"
        rcx = rax    // int64_t arg1
        fcn.140014d80 () // fcn.140014d80(0x0, 0x0)
        rbx = rax
        goto loc_0x140035678
        
    loc_0x140035678:
        // CODE XREF from fcn.140035630 @ 0x140035674(x)
        rax = rdi
        qword [rdi] = rbx
        rbx = qword [var_48h]
        rsp += 0x30
        rdi = pop ()
        return
        return rax;
    loc_0x140035676: // orphan
         // CODE XREF from fcn.140035630 @ 0x140035662(x)
         ebx = 0

    loc_0x140035678: // orphan
         // CODE XREF from fcn.140035630 @ 0x140035674(x)
         rax = rdi
         qword [rdi] = rbx
         rbx = qword [var_48h]
         rsp += 0x30
         rdi = pop ()
         return

        return rax;
    loc_0x1400356a0: // orphan
         rax = rip + 0x40aa1      // 0x140076148
         qword [rbx] = rax
         r8d = 0                  // LPCWSTR lpName
         edx = 0                  // BOOL bInitialOwner
         ecx = 0                  // LPSECURITY_ATTRIBUTES lpMutexAttributes
         qword [sym.imp.KERNEL32.dll_CreateMutexW] () // [0x14006a258:8]=0x91b24 reloc.KERNEL32.dll_CreateMutexW // "$\x1b\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CreateMutexW // HANDLE CreateMutexW(0, 0, 0)
         qword [rbx + 8] = rax
         rax = rdi
         qword [rdi] = rbx
         rbx = qword [var_48h]
         rsp += 0x30
         rdi = pop ()
         return

        return rax;
}

```

## ime_shared_memory

```c
===== FUNCTION ime_shared_memory 0x140035850 =====
[{"addr":5368928336,"name":"fcn.140035850","size":601,"is-pure":"false","realsz":601,"noreturn":false,"recursive":false,"stackframe":96,"calltype":"ms","cost":204,"cc":15,"bits":64,"type":"fcn","nbbs":22,"tracecov":0,"is-lineal":true,"ninstrs":134,"edges":29,"ebbs":4,"signature":"fcn.140035850 ();","minaddr":5368928336,"maxaddr":5368928937,"maxbbins":24,"midbbins":6.091,"ratbbins":3.940,"indegree":2,"outdegree":18,"nlocals":3,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.140035850 (int rcx, int rdx) {
    loc_0x140035850:
        // CALL XREF from fcn.140033250 @ 0x140033316(x)
        // CALL XREF from fcn.140034760 @ 0x1400349de(x)
        // NULL XREF from fcn.1400357a0 @ +0xaf(x)
        rsp -= 0x38
        rcx = rip + 0x67245 // 0x14009caa0 // LPCRITICAL_SECTION lpCriticalSection
        qword [sym.imp.KERNEL32.dll_InitializeCriticalSection] () // [0x14006a498:8]=0x91952 reloc.KERNEL32.dll_InitializeCriticalSection // "R\x19\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_InitializeCriticalSection // VOID InitializeCriticalSection(0x14009caa0)
        r9d = 0      // DWORD dwMaximumSizeHigh
        rax = rip + str.EVKeyIME_SharedMem_v1 // 0x140076160 // u"EVKeyIME_Sha\u0172"
        edx = 0      // LPSECURITY_ATTRIBUTES lpFileMappingAttributes
        qword [lpThreadId] = rax // LPCWSTR lpName
        dword [dwNumberOfBytesToMap] = 0x1038 // '8\x10' // [0x1038:4]=-1 // DWORD dwMaximumSizeLow
        r8d = r9 + 4
        rcx = rdx - 1 // HANDLE hFile
        qword [sym.imp.KERNEL32.dll_CreateFileMappingW] () // [0x14006a278:8]=0x91b68 reloc.KERNEL32.dll_CreateFileMappingW // "h\x1b\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CreateFileMappingW // HANDLE CreateFileMappingW(-1, 0, 0x00000000, 0, -1, -1)
        qword [0x14009ca78] = rax // [0x14009ca78:8]=0
        v = rax & rax
        if (v) goto loc_0x140035899 // likely
        goto loc_0x140035894;
    loc_0x140035899:
        // CODE XREF from fcn.140035850 @ 0x140035892(x)
        r9d = 0      // DWORD dwFileOffsetLow
        qword [var_30h] = rbx
        r8d = 0      // DWORD dwFileOffsetHigh
        qword [dwNumberOfBytesToMap] = 0x1038 // '8\x10' // [0x1038:8]=-1 // SIZE_T dwNumberOfBytesToMap
        edx = 0xf001f // '\x1f' // DWORD dwDesiredAccess // (pstr 0xffffffff) "\n"
        rcx = rax    // HANDLE hFileMappingObject
        qword [sym.imp.KERNEL32.dll_MapViewOfFile] () // [0x14006a268:8]=0x91b46 reloc.KERNEL32.dll_MapViewOfFile // "F\x1b\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_MapViewOfFile // LPVOID MapViewOfFile(0, -1, 0, 0, -1)
        qword [0x14009ca90] = rax // [0x14009ca90:8]=0
        v = rax & rax
        if (v) goto loc_0x1400358e9 // unlikely
        goto loc_0x1400358c7;
    loc_0x1400358e9:
        // CODE XREF from fcn.140035850 @ 0x1400358c5(x)
        edx = 0
        r8d = 0x1038 // '8\x10' // (pstr 0xffffffff) "\n"
        rcx = rax
        fcn.140049090 () // fcn.140049090(0x0, 0x0)
        r9 = rip + str.EVKeyIME_Request_v1 // 0x140076190 // LPCWSTR lpName
        r8d = 0      // BOOL bInitialState
        edx = 0      // BOOL bManualReset
        ecx = 0      // LPSECURITY_ATTRIBUTES lpEventAttributes
        qword [sym.imp.KERNEL32.dll_CreateEventW] () // [0x14006a500:8]=0x9185c reloc.KERNEL32.dll_CreateEventW // "\\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CreateEventW // HANDLE CreateEventW(0, 0, 0, "")
        r9 = rip + str.EVKeyIME_Response_v1 // 0x1400761b8 // u"EVKeyIME_Response_v1" // LPCWSTR lpName
        r8d = 0      // BOOL bInitialState
        edx = 0      // BOOL bManualReset
        qword [0x14009ca80] = rax // [0x14009ca80:8]=0
        ecx = 0      // LPSECURITY_ATTRIBUTES lpEventAttributes
        qword [sym.imp.KERNEL32.dll_CreateEventW] () // [0x14006a500:8]=0x9185c reloc.KERNEL32.dll_CreateEventW // "\\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CreateEventW // HANDLE CreateEventW(0, 0, 0, "E")
        rcx = qword [0x14009ca80] // [0x14009ca80:8]=0
        qword [0x14009ca88] = rax // [0x14009ca88:8]=0
        v = rcx & rcx
        if (!v) goto loc_0x1400359e0 // likely
        goto loc_0x14003593f;
    loc_0x1400359e0:
        // CODE XREFS from fcn.140035850 @ 0x140035939(x), 0x140035942(x)
        byte [0x14009ca98] = 0 // [0x14009ca98:1]=0
        v = rcx & rcx
        if (!v) goto loc_0x1400359f9 // likely
        goto loc_0x1400359ec;
    loc_0x1400359f9:
        // CODE XREF from fcn.140035850 @ 0x1400359ea(x)
        rax = qword [0x140096198] // [0x140096198:8]=0
        v = rax & rax
        if (!v) goto loc_0x140035a1a // likely
        goto loc_0x140035a05;
    loc_0x140035a1a:
        // CODE XREF from fcn.140035850 @ 0x140035a03(x)
        rax = qword [0x14009ca90] // [0x14009ca90:8]=0
        ebx = 0
        v = rax & rax
        if (!v) goto loc_0x140035a3f // likely
        goto loc_0x140035a28;
    loc_0x140035a3f:
        // CODE XREF from fcn.140035850 @ 0x140035a26(x)
        rax = qword [0x14009ca78] // [0x14009ca78:8]=0x140076160 str.EVKeyIME_SharedMem_v1 // u"EVKeyIME_Sha\u0172" str.EVKeyIME_SharedMem_v1
        v = rax & rax
        if (!v) goto loc_0x140035a62 // unlikely
        goto loc_0x140035a4b;
    loc_0x140035a62:
        // CODE XREF from fcn.140035850 @ 0x140035a49(x)
        v = rcx & rcx
        if (!v) goto loc_0x140035a74 // likely
        goto loc_0x140035a67;
    loc_0x140035a74:
        // CODE XREF from fcn.140035850 @ 0x140035a65(x)
        rax = qword [0x14009ca88] // [0x14009ca88:8]=0
        v = rax & rax
        if (!v) goto loc_0x140035a90 // likely
        goto loc_0x140035a80;
    loc_0x140035a90:
        // CODE XREF from fcn.140035850 @ 0x140035a7e(x)
        rcx = rip + 0x67009 // 0x14009caa0 // LPCRITICAL_SECTION lpCriticalSection
        qword [sym.imp.KERNEL32.dll_DeleteCriticalSection] () // [0x14006a480:8]=0x9199e reloc.KERNEL32.dll_DeleteCriticalSection // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_DeleteCriticalSection // VOID DeleteCriticalSection(0x14009caa0)
        rbx = qword [var_30h]
        al = 0
        rsp += 0x38
        return
        goto loc_0x140035948;
        return rax;
    loc_0x1400358c7: // orphan
         rcx = qword [0x14009ca78] // [0x14009ca78:8]=0 // HANDLE hObject
         qword [sym.imp.KERNEL32.dll_CloseHandle] () // [0x14006a510:8]=0x9183e reloc.KERNEL32.dll_CloseHandle // ">\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CloseHandle // BOOL CloseHandle(0)
         ebx = 0
         qword [0x14009ca78] = rbx // [0x14009ca78:8]=0
         al = 0
         rbx = qword [var_30h]
         rsp += 0x38
         return

        return rax;
    loc_0x14003593f: // orphan
         v = rax & rax
         if (!v) 
         goto loc_0x140035948
    loc_0x140035948: // orphan
         fcn.1400409a0 ()
         v = rax & rax
         if (!v) 
         goto loc_0x140035952
    loc_0x140035952: // orphan
         edx = dword [rax + 4]
         rcx = qword [0x14009ca90] // [0x14009ca90:8]=0
         dword [rcx + 0x20] = edx
         rcx = qword [0x14009ca90] // [0x14009ca90:8]=0
         edx = dword [rax + 0x14]
         dword [rcx + 0x28] = edx
         ecx = dword [rax + 0x1c]
         rax = qword [0x14009ca90] // [0x14009ca90:8]=0
         dword [rax + 0x2c] = ecx

    loc_0x140035979: // orphan
         // CODE XREF from fcn.140035850 @ 0x140035950(x)
         rcx = rip + 0x67000      // 0x14009c980 // uint32_t arg1
         fcn.140036830 ()         // fcn.140036830(0x0)
         ebx = 0
         v = al & al
         ecx = ebx
         rax = qword [0x14009ca90] // [0x14009ca90:8]=0
         cl = v != 0
         dword [rax + 0x24] = ecx
         qword [sym.imp.KERNEL32.dll_GetTickCount64] () // [0x14006a260:8]=0x91b34 reloc.KERNEL32.dll_GetTickCount64 // "4\x1b\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GetTickCount64 // ULONGLONG GetTickCount64(void)
         rcx = qword [0x14009ca90] // [0x14009ca90:8]=0
         r8 = rip + fcn.140035ab0 // 0x140035ab0 // "H\x83\xec(\x0f\xb6\x05\xddo\x06" // LPTHREAD_START_ROUTINE lpStartAddress
         qword [lpThreadId] = rbx // LPDWORD lpThreadId
         r9d = 0                  // LPVOID lpParameter
         edx = 0                  // SIZE_T dwStackSize
         dword [dwNumberOfBytesToMap] = ebx // DWORD dwCreationFlags
         qword [rcx + 0x30] = rax
         ecx = 0                  // LPSECURITY_ATTRIBUTES lpThreadAttributes
         byte [0x14009ca98] = 1   // [0x14009ca98:1]=0
         qword [sym.imp.KERNEL32.dll_CreateThread] () // [0x14006a528:8]=0x9180c reloc.KERNEL32.dll_CreateThread // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CreateThread // HANDLE CreateThread(0, 0, 0xdd05b60f28ec8348, 0, -1, -1)
         rbx = qword [var_30h]    // (pstr 0xffffffff) "\n"
         qword [0x140096198] = rax // [0x140096198:8]=0
         al = 1
         rsp += 0x38
         return

        return rax;
    loc_0x1400359ec: // orphan
         qword [sym.imp.KERNEL32.dll_SetEvent] () // [0x14006a520:8]=0x9181c reloc.KERNEL32.dll_SetEvent // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_SetEvent // BOOL SetEvent(0)
         rcx = qword [0x14009ca80] // [0x14009ca80:8]=0

    loc_0x140035a05: // orphan
         edx = 0x7d0              // 2000 // DWORD dwMilliseconds // (pstr 0xffffffff) "\n"
         rcx = rax                // HANDLE hHandle
         qword [sym.imp.KERNEL32.dll_WaitForSingleObject] () // [0x14006a518:8]=0x91828 reloc.KERNEL32.dll_WaitForSingleObject // "(\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_WaitForSingleObject // DWORD WaitForSingleObject(0, -1)
         rcx = qword [0x14009ca80] // [0x14009ca80:8]=0

    loc_0x140035a28: // orphan
         rcx = rax                // LPCVOID lpBaseAddress
         qword [sym.imp.KERNEL32.dll_UnmapViewOfFile] () // [0x14006a270:8]=0x91b56 reloc.KERNEL32.dll_UnmapViewOfFile // "V\x1b\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_UnmapViewOfFile // BOOL UnmapViewOfFile(0)
         rcx = qword [0x14009ca80] // [0x14009ca80:8]=0
         qword [0x14009ca90] = rbx // [0x14009ca90:8]=0

    loc_0x140035a4b: // orphan
         rcx = rax                // HANDLE hObject
         qword [sym.imp.KERNEL32.dll_CloseHandle] () // [0x14006a510:8]=0x9183e reloc.KERNEL32.dll_CloseHandle // ">\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CloseHandle // BOOL CloseHandle(0)
         rcx = qword [0x14009ca80] // [0x14009ca80:8]=0
         qword [0x14009ca78] = rbx // [0x14009ca78:8]=0

    loc_0x140035a67: // orphan
         qword [sym.imp.KERNEL32.dll_CloseHandle] () // [0x14006a510:8]=0x9183e reloc.KERNEL32.dll_CloseHandle // ">\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CloseHandle // BOOL CloseHandle(0)
         qword [0x14009ca80] = rbx // [0x14009ca80:8]=0

    loc_0x140035a80: // orphan
         rcx = rax                // HANDLE hObject
         qword [sym.imp.KERNEL32.dll_CloseHandle] () // [0x14006a510:8]=0x9183e reloc.KERNEL32.dll_CloseHandle // ">\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CloseHandle // BOOL CloseHandle(0)
         qword [0x14009ca88] = rbx // [0x14009ca88:8]=0

}

```

## startup_registry

```c
===== FUNCTION startup_registry 0x140036600 =====
[{"addr":5368931840,"name":"fcn.140036600","size":134691,"is-pure":"false","realsz":1019,"noreturn":false,"recursive":false,"stackframe":208,"calltype":"ms","cost":381,"cc":17,"bits":64,"type":"fcn","nbbs":32,"tracecov":0,"is-lineal":false,"ninstrs":278,"edges":47,"ebbs":1,"signature":"fcn.140036600 ();","minaddr":5368797312,"maxaddr":5368932003,"maxbbins":29,"midbbins":8.688,"ratbbins":3.338,"indegree":4,"outdegree":27,"nlocals":19,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.140036600 (int rcx, int rdx) {
    loc_0x140015880:
        // CODE XREF from fcn.140036600 @ 0x14003669e(x)
        byte [var_8h] = cl
        push (rbp)
        push (rsi)
        push (rdi)
        push (r12)
        push (r13)
        push (r14)
        push (r15)
        rbp = rsp
        rsp -= 0x70
        qword [var_bp_40h] = 0xfffffffffffffffe
        qword [var_c8h] = rbx
        rcx = var_bp_38h // PHANDLE arg1
        fcn.1400150c0 () // fcn.1400150c0(0x0)
        eax = dword [var_bp_30h]
        dword [var_48h] = eax
        v = eax & eax
        if (v) goto loc_0x1400158c6 // unlikely
        goto loc_0x1400158ba;
    loc_0x1400158c6:
        // CODE XREF from fcn.140036600 @ 0x1400158b8(x)
        rcx = var_18h // int64_t arg1
        fcn.140016270 () // fcn.140016270(0x0)
        rcx = qword [rax]
        rdi = rip + 0x556de // 0x14006afb8
        rdx = rdi
        v = rcx & rcx
        if (!zf) rdx = rcx // int64_t arg2
        rcx = var_bp_28h // int64_t arg1
        fcn.140038470 () // fcn.140038470(0x14006afb8, 0x0)
        rcx = qword [var_18h]
        fcn.14004bed8 () // fcn.14004bed8(0x14006afb8)
        r12d = 4
        ecx = r12d
        fcn.14004bee0 ()
        r15 = rax
        ecx = dword [0x14006d774] // [0x14006d774:4]=92
        dword [rax] = ecx
        rsi = rdi    // (pstr 0x00000000) "\"
        rcx = qword [var_bp_28h]
        v = rcx & rcx // (pstr 0x00000000) "\" // (pstr 0x00000000) "\"
        if (!zf) rsi = rcx
        rbx = dword [var_bp_20h] // (pstr 0xffffffff) "\n"
        ecx = r12 + 0xc
        r14d = r12 - 3 // (pstr 0x00000000) "\"
        r13d = 0     // (pstr 0x00000000) "\" // (pstr 0x00000000) "\"
        v = rsi & rsi // (pstr 0x00000000) "\"
        if (!v) goto loc_0x140015980 // unlikely
        goto loc_0x140015934;
    loc_0x140015980:
        // CODE XREFS from fcn.140036600 @ 0x140015932(x), 0x140015936(x)
        rsi = r13
        ebx = r13d
        r12d = r13d
        v = r14d & r14d // (pstr 0x00000000) "\" // (pstr 0x00000000) "\"
        if (v <= 0) goto loc_0x1400159b9 // likely
        goto loc_0x14001598e;
    loc_0x1400159b9:
        // CODE XREFS from fcn.140036600 @ 0x14001598c(x), 0x140015991(x)
        r13d = rbx + 4 // (pstr 0x00000000) "\"
        ecx = r13 + 1 // (pstr 0x00000000) "\"
        v = ecx - r12d // (pstr 0x00000000) "\"
        if (v <= 0) goto loc_0x1400159ef // unlikely
        goto loc_0x1400159c6;
    loc_0x1400159ef:
        // CODE XREF from fcn.140036600 @ 0x1400159c4(x)
        rax = ebx
        rcx = rsi + rax*2
        xmm0 = qword [str..lnk] // [0x14006d900:8]=0x6b006e006c002e // u".lnk"
        qword [rcx] = xmm0
        eax = word [0x14006d908] // [0x14006d908:2]=0
        word [rcx + 8] = ax
        r12 = rdi
        v = rsi & rsi
        if (!zf) r12 = rsi
        v = r12 & r12
        if (!v) goto loc_0x140015a94 // likely
        goto loc_0x140015a1c;
    loc_0x140015a94:
        // CODE XREFS from fcn.140036600 @ 0x140015a1a(x), 0x140015a1f(x)
        rbx = qword [var_bp_38h]
        r14d = 0
        
    loc_0x140015a9b:
        // CODE XREF from fcn.140036600 @ 0x140015a92(x)
        rcx = rsi
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        rcx = r15
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        rcx = qword [var_bp_28h]
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        v = byte [var_8h] - 0
        if (!v) goto loc_0x140015ba1 // likely
        goto loc_0x140015abe;
        goto loc_0x140015a21;
        goto loc_0x140015993;
        goto loc_0x140015938;
        return rax;
    loc_0x140015934: // orphan
         v = ebx & ebx
         if (v <= 0) 
         goto loc_0x140015938
    loc_0x140015938: // orphan
         r14d = rbx + 1
         eax = r14 + 1            // reloc.OLEAUT32.dll_SysReAllocString
         v = eax - 2              // reloc.OLEAUT32.dll_SysReAllocString
         if (v <= 0) 
         goto loc_0x140015945
    loc_0x140015945: // orphan
         v = eax - r12d
         cmovle eax r12d
         v = eax - ecx
         if (v < 0) eax = ecx
         rdx = eax
         rdx += rdx
         rcx = r15
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) r15 = rax

    loc_0x140015966: // orphan
         // CODE XREF from fcn.140036600 @ 0x140015943(x)
         r8 = rbx
         r8 += r8
         rcx = r15 + 2            // reloc.OLEAUT32.dll_SysReAllocString
         rdx = rsi
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rax = r14d
         word [r15 + rax*2] = r13w

    loc_0x14001598e: // orphan
         v = r15 & r15
         if (!v) 
         goto loc_0x140015993
    loc_0x140015993: // orphan
         r12d = r14 + 1
         rbx = r12d
         rbx += rbx               // reloc.OLEAUT32.dll_SysReAllocString
         rcx = rbx                // reloc.OLEAUT32.dll_SysReAllocString
         fcn.14004bee0 ()
         rsi = rax
         r8 = rbx                 // reloc.OLEAUT32.dll_SysReAllocString
         rdx = r15
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         ebx = r14d

    loc_0x1400159c6: // orphan
         eax = r12 + r12
         v = eax - ecx
         cmovge ecx eax
         v = ecx - 0x10
         eax = 0x10
         if (v < 0) ecx = eax
         rdx = ecx
         rdx += rdx
         rcx = rsi
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) rsi = rax

    loc_0x140015a1c: // orphan
         v = r13d & r13d
         if (v <= 0) 
         goto loc_0x140015a21
    loc_0x140015a21: // orphan
         r14d = dword [var_48h]
         r14d++
         r14d += r13d
         eax = dword [var_2ch]
         rbx = qword [var_bp_38h]
         v = r14d - eax
         if (v <= 0) 
         goto loc_0x140015a37
    loc_0x140015a37: // orphan
         eax += eax
         v = eax - r14d
         cmovge r14d eax
         v = r14d - 0x10
         eax = 0x10
         if (v < 0) r14d = eax
         rdx = r14d
         rdx += rdx
         rcx = rbx
         fcn.14004bee8 ()
         v = rax & rax
         if (!v) 
    loc_0x140015a60: // orphan
         rbx = rax
         qword [var_bp_38h] = rax
         dword [var_2ch] = r14d

    loc_0x140015a6b: // orphan
         // CODE XREFS from fcn.140036600 @ 0x140015a35(x), 0x140015a5e(x)
         r8 = r13d
         r8 += r8
         r14 = dword [var_48h]
         rcx = rbx + r14*2
         rdx = r12
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         eax = r14 + r13
         dword [var_bp_30h] = eax
         cdqe
         r14d = 0
         word [rbx + rax*2] = r14w
         
    loc_0x140015a9b: // orphan
         // CODE XREF from fcn.140036600 @ 0x140015a92(x)
         rcx = rsi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = r15
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = qword [var_bp_28h]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         v = byte [var_8h] - 0
         if (!v) 
         goto loc_0x140015abe
    loc_0x140015abe: // orphan
         rcx = rdi
         v = rbx & rbx
         if (!zf) rcx = rbx       // LPCWSTR pszPath
         qword [sym.imp.SHLWAPI.dll_PathFileExistsW] () // [0x14006a630:8]=0x91764 reloc.SHLWAPI.dll_PathFileExistsW // "d\x17\t" // (pstr 0xffffffff) "\n" reloc.SHLWAPI.dll_PathFileExistsW // BOOL PathFileExistsW(0)
         v = eax - 1
         if (!v) 
         goto loc_0x140015ad7
    loc_0x140015ad7: // orphan
         edx = 0                  // DWORD dwCoInit
         ecx = 0                  // LPVOID pvReserved
         qword [sym.imp.ole32.dll_CoInitializeEx] () // [0x14006abd0:8]=0x92872 reloc.ole32.dll_CoInitializeEx // "r(\t" // (pstr 0xffffffff) "\n" reloc.ole32.dll_CoInitializeEx // HRESULT CoInitializeEx(0, 0)
         qword [var_48h] = r14
         rax = var_48h
         qword [ppv] = rax        // LPVOID *ppv
         r9 = rip + 0x67c6b       // 0x14007d760 // REFIID riid
         edx = 0                  // LPUNKNOWN pUnkOuter
         r8d = rdx + 1
         rcx = rip + 0x67c7e      // 0x14007d780 // REFCLSID rclsid
         qword [sym.imp.ole32.dll_CoCreateInstance] () // [0x14006abb8:8]=0x9285e reloc.ole32.dll_CoCreateInstance // "^(\t" // (pstr 0xffffffff) "\n" reloc.ole32.dll_CoCreateInstance // HRESULT CoCreateInstance(0x14007d780, 0, 0x00000000, 0x00000000000214f9, -1)
         v = eax & eax
         js 0x140015b99           // unlikely

         goto loc_0x140015b10
    loc_0x140015b10: // orphan
         qword [var_bp_50h] = r14
         rcx = var_18h            // int64_t arg1
         fcn.140016270 ()         // fcn.140016270(0x0)
         rcx = qword [rax]
         rdx = rdi
         v = rcx & rcx
         if (!zf) rdx = rcx
         rcx = qword [var_48h]
         rax = qword [rcx]
         qword [rax + 0xa0] ()    // 0xa0(0x0, 0x0, 0x0, 0x0)
         rcx = qword [var_18h]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = qword [var_48h]
         rax = qword [rcx]
         rdx = rip + 0x57dc0      // u"EVKey - Vietnamese Keyboard" // 0x14006d910
         qword [rax + 0x38] ()    // 0x38(0x0, 0x0, 0x14006d910, 0x0)
         rcx = qword [var_48h]
         rax = qword [rcx]
         r8 = var_bp_50h
         rdx = rip + 0x67c0b      // 0x14007d770
         qword [rax] ()           // 0x140015b67(0x0, 0x0, 0x14007d770, 0x0)
         v = eax & eax
         js 0x140015b8f           // unlikely

         goto loc_0x140015b6b
    loc_0x140015b6b: // orphan
         v = rbx & rbx
         if (!zf) rdi = rbx
         rcx = qword [var_bp_50h]
         rax = qword [rcx]
         r8d = 1
         rdx = rdi
         qword [rax + 0x30] ()    // 0x30(0x0, 0x0, 0x0, 0x0)
         rcx = qword [var_bp_50h]
         rax = qword [rcx]
         qword [rax + 0x10] ()    // 0x10(0x0, 0x0, 0x0, 0x0)

    loc_0x140015b8f: // orphan
         // CODE XREF from fcn.140036600 @ 0x140015b69(x)
         rcx = qword [var_48h]
         rax = qword [rcx]
         qword [rax + 0x10] ()    // 0x10(0x0, 0x0, 0x0, 0x0)

    loc_0x140015b99: // orphan
         // CODE XREF from fcn.140036600 @ 0x140015b0a(x)
         qword [sym.imp.ole32.dll_CoUninitialize] () // [0x14006abc0:8]=0x9284c reloc.ole32.dll_CoUninitialize // "L(\t" // (pstr 0xffffffff) "\n" reloc.ole32.dll_CoUninitialize // void CoUninitialize(void)
         
    loc_0x140015ba1: // orphan
         // CODE XREF from fcn.140036600 @ 0x140015ab8(x)
         v = rbx & rbx
         if (!zf) rdi = rbx
         rcx = rdi                // LPCWSTR lpFileName
         qword [sym.imp.KERNEL32.dll_DeleteFileW] () // [0x14006a4d8:8]=0x918be reloc.KERNEL32.dll_DeleteFileW // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_DeleteFileW // BOOL DeleteFileW(0)

    loc_0x140015bb1: // orphan
         // CODE XREFS from fcn.140036600 @ 0x140015ad1(x), 0x140015b9f(x)
         dil = 1

    loc_0x140015bb4: // orphan
         // CODE XREF from fcn.140036600 @ 0x1400158c1(x)
         rcx = rbx
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         eax = dil
         rbx = qword [var_c8h]
         rsp += 0x70
         r15 = pop ()
         r14 = pop ()
         r13 = pop ()
         r12 = pop ()
         rdi = pop ()
         rsi = pop ()             // (pstr 0xffffffff) "\n" // (pstr 0xffffffff) "\n"
         rbp = pop ()             // "\n"
         return

        return rax;
    loc_0x140036600: // orphan
           // CALL XREFS from fcn.140036850 @ 0x1400368cf(x), 0x1400368e7(x), 0x140036911(x), 0x140036928(x)
         qword [var_8h] = rbx
         push (rdi)
         rsp -= 0x60
         rax = var_58h
         edi = 0
         qword [var_40h] = rax
         ebx = dl
         rax = hKey
         r9d = 0
         qword [var_38h] = rax
         rdx = rip + 0x3fed3      // u"Software\Microsoft\Windows\CurrentVersion\Run" // 0x140076500 // LPCWSTR lpSubKey
         qword [var_30h] = rdi
         r8d = 0                  // DWORD Reserved
         dword [lpcbData] = 3
         rcx = 0xffffffff80000001 // HKEY hKey
         dword [lpData] = edi
         qword [sym.imp.ADVAPI32.dll_RegCreateKeyExW] () // [0x14006a030:8]=0x916b6 reloc.ADVAPI32.dll_RegCreateKeyExW // (pstr 0xffffffff) "\n" reloc.ADVAPI32.dll_RegCreateKeyExW // LSTATUS RegCreateKeyExW(-1, "S", 0, 0, -1, -1, -1, -1, -1)
         v = eax & eax
         if (v) 
         goto loc_0x140036652
    loc_0x140036652: // orphan
         rcx = qword [hKey]       // HKEY hKey
         rdx = rip + 0x3ff12      // u"EVKey" // 0x140076570 // LPCWSTR lpValueName
         qword [sym.imp.ADVAPI32.dll_RegDeleteValueW] () // [0x14006a038:8]=0x916c8 reloc.ADVAPI32.dll_RegDeleteValueW // (pstr 0xffffffff) "\n" reloc.ADVAPI32.dll_RegDeleteValueW // LSTATUS RegDeleteValueW(0, "E")
         rcx = qword [hKey]       // HKEY hKey
         rdx = rip + 0x3ff10      // u"EVKey" // 0x140076580 // LPCWSTR lpValueName
         r9d = 0                  // LPDWORD lpType
         qword [lpcbData] = rdi   // LPDWORD lpcbData
         r8d = 0                  // LPDWORD lpReserved
         qword [lpData] = rdi     // LPBYTE lpData
         qword [sym.imp.ADVAPI32.dll_RegQueryValueExW] () // [0x14006a010:8]=0x91666 reloc.ADVAPI32.dll_RegQueryValueExW // "f\x16\t" // (pstr 0xffffffff) "\n" reloc.ADVAPI32.dll_RegQueryValueExW // LSTATUS RegQueryValueExW(0, "E", 0, 0, -1, -1)
         rcx = qword [hKey]       // HKEY hKey
         qword [sym.imp.ADVAPI32.dll_RegCloseKey] () // [0x14006a000:8]=0x91648 reloc.ADVAPI32.dll_RegCloseKey // "H\x16\t" // (pstr 0xffffffff) "\n" reloc.ADVAPI32.dll_RegCloseKey // LSTATUS RegCloseKey(0)

    loc_0x140036691: // orphan
         // CODE XREF from fcn.140036600 @ 0x140036650(x)
         ecx = bl
         rbx = qword [var_8h]
         rsp += 0x60
         rdi = pop ()
         
}

```

## clipboard_controller

```c
===== FUNCTION clipboard_controller 0x14002b4e0 =====
[{"addr":5368886496,"name":"fcn.14002b4e0","size":2225,"is-pure":"false","realsz":2225,"noreturn":false,"recursive":false,"stackframe":384,"calltype":"ms","cost":837,"cc":59,"bits":64,"type":"fcn","nbbs":102,"tracecov":0,"is-lineal":true,"ninstrs":555,"edges":159,"ebbs":1,"signature":"fcn.14002b4e0 (int64_t arg1, uint32_t arg2, int64_t arg3, LPCWSTR arg4, uint32_t arg_80h, int64_t arg_88h);","minaddr":5368886496,"maxaddr":5368888721,"maxbbins":33,"midbbins":5.441,"ratbbins":6.065,"indegree":3,"outdegree":77,"nlocals":34,"nargs":6,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.14002b4e0 (int64_t arg1, uint32_t arg2, int64_t arg3, LPCWSTR arg4, uint32_t arg_80h, int64_t arg_88h) {
    loc_0x14002b4e0:
        // CALL XREF from fcn.14002d430 @ 0x14002d51f(x)
        // CALL XREFS from fcn.140033450 @ 0x14003388e(x), 0x140033d6b(x)
        push (rbp)
        push (rsi)
        push (rdi)
        push (r12)
        push (r13)
        push (r14)
        push (r15)
        rbp = rsp - 0x20
        rsp -= 0x120
        qword [var_40h] = 0xfffffffffffffffe
        qword [var_178h] = rbx
        rax = qword [0x14009a9e8] // [0x14009a9e8:8]=0x2b992ddfa200
        rax ^= rsp
        qword [var_18h] = rax
        r15d = r9d   // arg4
        dword [lpText] = r9d // arg4
        qword [var_48h] = r8 // arg3
        dword [var_44h] = edx // arg2
        r12d = ecx   // arg1
        rsi = qword [arg_88h]
        qword [hWnd] = rsi
        ecx = 0
        r14d = ecx
        r13d = ecx
        edi = ecx
        dword [var_sp_48h] = ecx
        dword [r8] = ecx
        qword [sym.imp.USER32.dll_GetClipboardOwner] () // [0x14006a6e8:8]=0x9228e reloc.USER32.dll_GetClipboardOwner // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetClipboardOwner // HWND GetClipboardOwner(void)
        rbx = rax
        rcx = qword [0x140096148] // [0x140096148:8]=0 // HWND hWndNewOwner
        qword [sym.imp.USER32.dll_OpenClipboard] () // [0x14006a868:8]=0x91d02 reloc.USER32.dll_OpenClipboard // (pstr 0xffffffff) "\n" reloc.USER32.dll_OpenClipboard // BOOL OpenClipboard(0)
        v = eax & eax
        if (v) goto loc_0x14002b5ea // likely
        goto loc_0x14002b564;
    loc_0x14002b5ea:
        // CODE XREF from fcn.14002b4e0 @ 0x14002b55e(x)
        rcx = var_8h // int64_t arg1
        fcn.140043470 () // fcn.140043470(0x0)
        rcx = var_28h // int64_t arg1
        fcn.140043490 () // fcn.140043490(0x0)
        eax = byte [0x14009c9ea] // [0x14009c9ea:1]=255
        dword [var_1ch] = eax
        eax = byte [0x14009c9eb] // [0x14009c9eb:1]=255
        dword [var_20h] = eax
        eax = byte [0x14009c9ec] // [0x14009c9ec:1]=255
        dword [var_18h_2] = eax
        eax = byte [0x14009c9ed] // [0x14009c9ed:1]=255
        dword [var_14h] = eax
        eax = byte [0x14009c9ee] // [0x14009c9ee:1]=255
        dword [var_10h] = eax
        rcx = var_28h // int64_t arg1
        fcn.1400434b0 () // fcn.1400434b0(0x0)
        esi = 0
        dword [var_78h] = esi
        r15d = rsi + 1
        v = rbx - qword [0x140096148] // [0x140096148:8]=0
        if (v) goto loc_0x14002b6c3 // unlikely
        goto loc_0x14002b64a;
    loc_0x14002b6c3:
        // CODE XREFS from fcn.14002b4e0 @ 0x14002b648(x), 0x14002b65c(x)
        ebx = dword [uFormat]
        qword [var_bp_58h] = rbx
        
    loc_0x14002b6cb:
        // CODE XREFS from fcn.14002b4e0 @ 0x14002b67d(x), 0x14002b6b0(x), 0x14002b6c1(x)
        byte [var_68h] = sil
        fcn.14002be70 ()
        ecx = eax    // UINT uFormat
        qword [sym.imp.USER32.dll_GetClipboardData] () // [0x14006a880:8]=0x91d38 reloc.USER32.dll_GetClipboardData // "8\x1d\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetClipboardData // HANDLE GetClipboardData(0)
        r15 = rax
        eax = 0
        v = r15 & r15
        if (!v) goto loc_0x14002b7e8 // likely
        goto loc_0x14002b6eb;
        goto loc_0x14002b65e;
        goto loc_0x14002b56c;
        return rax;
    loc_0x14002b56c:
        rax = rip + 0x499f5 // u"Cannot read the Clipboard" // 0x140074f68
        rdx = rip + 0x49a6e // u"Kh\xf4ng th\u1ec3 \u0111\u1ecdc \u0111\u01b0\u1ee3c Clipboard" // 0x140074fe8
        v = r15d - 1
        if (!zf) rdx = rax // LPCWSTR lpText // u"Cannot read the Clipboard"
        r9d = rdi + 0x30
        r8 = rip + 0x49a9b // u"EVKey Converter" // 0x140075028 // LPCWSTR lpCaption
        rcx = rsi    // HWND hWnd
        qword [sym.imp.USER32.dll_MessageBoxW] () // [0x14006aa48:8]=0x91f90 reloc.USER32.dll_MessageBoxW // (pstr 0xffffffff) "\n" reloc.USER32.dll_MessageBoxW // int MessageBoxW(0, "C", "E", 0x00000000)
        
    loc_0x14002b596:
        // CODE XREF from fcn.14002b4e0 @ 0x14002b56a(x)
        ebx = edi
        return rax;
    loc_0x14002b596: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b56a(x)
         ebx = edi

    loc_0x14002b598: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002ba0a(x), 0x14002bd40(x), 0x14002bd6f(x), 0x14002bd8c(x)
         rcx = var_8h             // int64_t arg1
         fcn.1400434b0 ()         // fcn.1400434b0(0x0)
         v = r13 & r13
         if (!v) 
         goto loc_0x14002b5a6
    loc_0x14002b5a6: // orphan
         v = rdi & rdi
         if (!v) 
         goto loc_0x14002b5ab
    loc_0x14002b5ab: // orphan
         v = rdi - r13
         if (!v) 
    loc_0x14002b5b0: // orphan
         rcx = rdi
         fcn.1400474e4 ()

    loc_0x14002b5b8: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b5a9(x), 0x14002b5ae(x)
         rcx = r14                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)

    loc_0x14002b5c1: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b5a4(x)
         eax = ebx
         rcx = qword [var_18h]
         rcx ^= rsp
         fcn.140047380 ()         // fcn.140047380(0x0)
         rbx = qword [var_178h]
         rsp += 0x120
         r15 = pop ()
         r14 = pop ()
         r13 = pop ()
         r12 = pop ()
         rdi = pop ()
         rsi = pop ()
         rbp = pop ()
         return

        return rax;
    loc_0x14002b64a: // orphan
         ecx = dword [0x140095f2c] // [0x140095f2c:4]=1 // UINT uFormat
         qword [sym.imp.USER32.dll_GetClipboardData] () // [0x14006a880:8]=0x91d38 reloc.USER32.dll_GetClipboardData // "8\x1d\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetClipboardData // HANDLE GetClipboardData(0x00000000)
         r14 = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14002b65e
    loc_0x14002b65e: // orphan
         rcx = rax                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalSize] () // [0x14006a548:8]=0x917d2 reloc.KERNEL32.dll_GlobalSize // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalSize // SIZE_T GlobalSize(0)
         rbx = rax
         qword [var_bp_58h] = rax
         rcx = r14                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         r13 = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14002b67f
    loc_0x14002b67f: // orphan
         v = ebx - 0xc
         if (((unsigned) v) < 0) 
         goto loc_0x14002b684
    loc_0x14002b684: // orphan
         rcx = ebx
         xmm0 = qword [rcx + rax - 0xc]
         qword [var_80h] = xmm0
         ecx = dword [rcx + rax - 4]
         dword [var_bp_78h] = ecx
         eax = 0xf1e3             // (pstr 0xffffffff) "\n"
         v = cx - ax
         if (v) 
    loc_0x14002b6a3: // orphan
         dword [var_78h] = r15d
         r12 = qword [var_80h]
         r12 >>>= 0x20
         
    loc_0x14002b6b2: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b682(x), 0x14002b6a1(x)
         rcx = r14                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)
         r13 = rsi
         r14 = rsi
         
    loc_0x14002b6cb: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b67d(x), 0x14002b6b0(x), 0x14002b6c1(x)
         byte [var_68h] = sil
         fcn.14002be70 ()
         ecx = eax                // UINT uFormat
         qword [sym.imp.USER32.dll_GetClipboardData] () // [0x14006a880:8]=0x91d38 reloc.USER32.dll_GetClipboardData // "8\x1d\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetClipboardData // HANDLE GetClipboardData(0)
         r15 = rax
         eax = 0
         v = r15 & r15
         if (!v) 
         goto loc_0x14002b6eb
    loc_0x14002b6eb: // orphan
         rcx = r15                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         edx = 0
         qword [dwBytes] = rdx
         qword [var_bp_70h] = rdx
         qword [var_bp_68h] = rdx
         v = rax & rax
         if (!v) 
         goto loc_0x14002b70c
    loc_0x14002b70c: // orphan
         r9d = dword [var_44h]
         r8d = r12d               // int64_t arg_78h
         rdx = rax                // int64_t arg_70h
         rcx = var_bp_38h         // uint32_t arg_68h
         fcn.14002dab0 ()         // fcn.14002dab0(0x0, 0x0, 0x0)
         rbx = rax
         eax = 0
         qword [uFormat] = rax
         rax = var_bp_70h
         v = rax - rbx
         if (!v) 
         goto loc_0x14002b733
    loc_0x14002b733: // orphan
         ecx = 0
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rax = qword [rbx]
         qword [dwBytes] = rax
         ecx = 0
         qword [rbx] = rcx
         qword [rbx + 8] = rcx
         rbx = rax
         qword [uFormat] = rax
         
         goto loc_0x14002b755
    loc_0x14002b755: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b731(x)
         rbx = qword [uFormat]

    loc_0x14002b75a: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b753(x)
         rcx = qword [var_bp_38h]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = r15                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)
         rcx = rip + 0x489d5      // 0x140074148
         v = rbx & rbx
         if (!zf) rcx = rbx
         rax = 0xffffffffffffffff

    loc_0x14002b781: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b788(x)
         rax++
         v = byte [rcx + rax] - 0
         if (v) 
         goto loc_0x14002b78a
    loc_0x14002b78a: // orphan
         v = eax & eax
         if (!v) 
    loc_0x14002b78e: // orphan
         r15 = eax
         rdx = r15 + 1            // SIZE_T dwBytes
         ecx = 2                  // UINT uFlags // reloc.OLEAUT32.dll_SysReAllocString
         qword [sym.imp.KERNEL32.dll_GlobalAlloc] () // [0x14006a550:8]=0x917c4 reloc.KERNEL32.dll_GlobalAlloc // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalAlloc // HGLOBAL GlobalAlloc(0x00000000, 1)
         rbx = rax
         v = rax & rax
         if (!v) 
    loc_0x14002b7a8: // orphan
         rcx = rax                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         rcx = qword [uFormat]
         v = rcx & rcx
         rdx = rip + 0x48988      // 0x140074148
         if (!zf) rdx = rcx
         r8 = r15 + 1
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x140074148)
         rcx = rbx                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)
         rsi = rbx

    loc_0x14002b7dc: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b706(x), 0x14002b78c(x), 0x14002b7a6(x)
         rcx = qword [dwBytes]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         eax = 0

    loc_0x14002b7e8: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b6e5(x)
         v = rsi & rsi
         if (v) 
         goto loc_0x14002b7f1
    loc_0x14002b7f1: // orphan
         rsi = rax
         rbx = rax
         qword [var_bp_70h] = rax
         qword [var_bp_68h] = 0
         fcn.14002be00 ()
         r15d = eax
         v = eax & eax
         if (!v) 
         goto loc_0x14002b813
    loc_0x14002b813: // orphan
         ecx = eax                // UINT format
         qword [sym.imp.USER32.dll_IsClipboardFormatAvailable] () // [0x14006a6e0:8]=0x922a2 reloc.USER32.dll_IsClipboardFormatAvailable // (pstr 0xffffffff) "\n" reloc.USER32.dll_IsClipboardFormatAvailable // BOOL IsClipboardFormatAvailable(0)
         v = eax & eax
         if (!v) 
    loc_0x14002b823: // orphan
         ecx = r15d               // UINT uFormat
         qword [sym.imp.USER32.dll_GetClipboardData] () // [0x14006a880:8]=0x91d38 reloc.USER32.dll_GetClipboardData // "8\x1d\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetClipboardData // HANDLE GetClipboardData(0)
         r15 = rax
         qword [hMem] = rax
         rcx = rax                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalSize] () // [0x14006a548:8]=0x917d2 reloc.KERNEL32.dll_GlobalSize // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalSize // SIZE_T GlobalSize(0)
         qword [uFormat] = rax
         rcx = r15                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         r15 = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14002b856
    loc_0x14002b856: // orphan
         dword [dwBytes] = 0xffffffff // [0xffffffff:4]=0xa0a000a // -1
         rax = dwBytes            // (pstr 0xffffffff) "\n"
         qword [var_sp_28h] = rax
         rax = var_bp_70h
         qword [var_sp_20h] = rax
         r9d = dword [uFormat]    // int64_t arg4
         r8 = r15                 // int64_t arg3
         edx = dword [var_44h]    // int64_t arg2
         ecx = r12d               // int64_t arg1
         fcn.14002c860 ()         // fcn.14002c860(0x0, 0x0, 0x0, 0x0)
         ebx = eax
         v = eax & eax
         if (!v) 
         goto loc_0x14002b88b
    loc_0x14002b88b: // orphan
         rax = dwBytes            // (pstr 0xffffffff) "\n"
         qword [var_sp_28h] = rax
         rax = var_bp_70h
         qword [var_sp_20h] = rax
         r9d = dword [uFormat]    // int64_t arg4
         r8 = r15                 // int64_t arg3
         edx = dword [var_44h]    // int64_t arg2
         ecx = r12d               // int64_t arg1
         fcn.14002c860 ()         // fcn.14002c860(0x0, 0x0, 0x0, 0x0)
         ebx = eax

    loc_0x14002b8b4: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b889(x)
         rcx = qword [hMem]       // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)
         v = ebx & ebx
         if (v) 
         goto loc_0x14002b8c2
    loc_0x14002b8c2: // orphan
         rdx = dword [dwBytes]    // SIZE_T dwBytes
         ecx = 0x2002             // UINT uFlags // (pstr 0xffffffff) "\n"
         qword [sym.imp.KERNEL32.dll_GlobalAlloc] () // [0x14006a550:8]=0x917c4 reloc.KERNEL32.dll_GlobalAlloc // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalAlloc // HGLOBAL GlobalAlloc(-1, -1)
         rsi = rax
         v = rax & rax
         if (!v) 
    loc_0x14002b8da: // orphan
         rcx = rax                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         r15 = rax
         v = rax & rax
         if (!v) 
    loc_0x14002b8eb: // orphan
         rbx = qword [var_bp_70h]
         rcx = rbx                // int64_t arg1
         fcn.14002cbd0 ()         // fcn.14002cbd0(0x0)
         r8 = dword [dwBytes]
         rdx = rbx
         rcx = r15
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rcx = rsi                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)
         
    loc_0x14002b912: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b8c0(x), 0x14002b8d8(x), 0x14002b8e9(x)
         rbx = qword [var_bp_70h]

    loc_0x14002b916: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b850(x), 0x14002b910(x)
         rcx = rbx
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         eax = dword [var_68h]    // "\n"
         eax = al
         v = rsi & rsi
         r15d = 1
         if (!zf) eax = r15d
         dword [var_68h] = eax
         
         goto loc_0x14002b938
    loc_0x14002b938: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b80d(x), 0x14002b81d(x)
         ecx = 0
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         eax = 0
         esi = eax

    loc_0x14002b943: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b7eb(x)
         r15d = 1

    loc_0x14002b949: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b936(x)
         eax = 0xc0000011         // '\x11' // (pstr 0xffffffff) "\n"
         edx = dword [var_44h]
         v = edx - 0x1f
         if (((unsigned) v) > 0) 
         goto loc_0x14002b957
    loc_0x14002b957: // orphan
         bt eax edx
         jae 0x14002b960          // likely

    loc_0x14002b95c: // orphan
         cl = 1
         
         goto loc_0x14002b960
    loc_0x14002b960: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b955(x), 0x14002b95a(x)
         cl = 0

    loc_0x14002b962: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b95e(x)
         byte [var_sp_40h] = cl
         eax = r15d
         ebx = 0xd
         v = cl & cl
         if (!zf) eax = ebx
         dword [uFormat] = eax
         v = dword [var_78h] - 0
         if (v) 
         goto loc_0x14002b982
    loc_0x14002b982: // orphan
         ecx = ebx                // UINT format
         qword [sym.imp.USER32.dll_IsClipboardFormatAvailable] () // [0x14006a6e0:8]=0x922a2 reloc.USER32.dll_IsClipboardFormatAvailable // (pstr 0xffffffff) "\n" reloc.USER32.dll_IsClipboardFormatAvailable // BOOL IsClipboardFormatAvailable(0)
         v = eax & eax
         if (!v) 
         goto loc_0x14002b98e
    loc_0x14002b98e: // orphan
         v = r12d - 0x1f
         if (((unsigned) v) > 0) 
    loc_0x14002b994: // orphan
         eax = 0xc0400011         // '\x11' // (pstr 0xffffffff) "\n"
         bt eax r12d
         if (((unsigned) v) < 0) 
         goto loc_0x14002b99f
    loc_0x14002b99f: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b98c(x), 0x14002b992(x)
         ebx = r15d

    loc_0x14002b9a2: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b99d(x)
         ecx = ebx                // UINT uFormat
         qword [sym.imp.USER32.dll_GetClipboardData] () // [0x14006a880:8]=0x91d38 reloc.USER32.dll_GetClipboardData // "8\x1d\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetClipboardData // HANDLE GetClipboardData(0)
         r14 = rax
         v = rax & rax
         if (v) 
         goto loc_0x14002b9b2
    loc_0x14002b9b2: // orphan
         ebx = r15d
         ecx = r15d               // UINT uFormat
         qword [sym.imp.USER32.dll_GetClipboardData] () // [0x14006a880:8]=0x91d38 reloc.USER32.dll_GetClipboardData // "8\x1d\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetClipboardData // HANDLE GetClipboardData(0)
         r14 = rax
         v = rax & rax
         if (v) 
    loc_0x14002b9c6: // orphan
         v = rsi & rsi
         if (v) 
    loc_0x14002b9cb: // orphan
         v = dword [arg_80h] - esi
         if (!v) 
         goto loc_0x14002b9d3
    loc_0x14002b9d3: // orphan
         rax = rip + str.Cannot_read_the_Clipboard // 0x140075088 // u"Cannot read the Clipboard"
         rdx = rip + 0x49667      // u"Kh\xf4ng th\u1ec3 \u0111\u1ecdc \u0111\u01b0\u1ee3c Clipboard\u2a00\x04" // 0x140075048
         v = dword [lpText] - 1
         if (!zf) rdx = rax       // u"Cannot read the Clipboard" str.Cannot_read_the_Clipboard
         r9d = r14 + 0x30
         r8 = rip + 0x496cb       // u"EVKey Converter" // 0x1400750c0

    loc_0x14002b9f5: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002baff(x), 0x14002bb87(x)
         rcx = qword [hWnd]       // HWND hWnd
         qword [sym.imp.USER32.dll_MessageBoxW] () // [0x14006aa48:8]=0x91f90 reloc.USER32.dll_MessageBoxW // (pstr 0xffffffff) "\n" reloc.USER32.dll_MessageBoxW // int MessageBoxW(0x6c894808245c8948, 0, 0, 0)

    loc_0x14002ba00: // orphan
         // XREFS: CODE 0x14002b9d1  CODE 0x14002ba95  CODE 0x14002ba9e   // XREFS: CODE 0x14002bad7  CODE 0x14002bb13  CODE 0x14002bb5f  
         qword [sym.imp.USER32.dll_CloseClipboard] () // [0x14006a870:8]=0x91d12 reloc.USER32.dll_CloseClipboard // (pstr 0xffffffff) "\n" reloc.USER32.dll_CloseClipboard // BOOL CloseClipboard(void)
         ebx = dword [var_sp_48h]
         
         goto loc_0x14002ba0f
    loc_0x14002ba0f: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002b9b0(x), 0x14002b9c4(x), 0x14002b9c9(x)
         rcx = r14                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalSize] () // [0x14006a548:8]=0x917d2 reloc.KERNEL32.dll_GlobalSize // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalSize // SIZE_T GlobalSize(0)
         r15d = eax
         rcx = r14                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         r13 = rax
         v = r12d - 0x1f
         if (((unsigned) v) > 0) 
         goto loc_0x14002ba2d
    loc_0x14002ba2d: // orphan
         eax = 0xc0000011         // '\x11' // (pstr 0xffffffff) "\n"
         bt eax r12d
         if (((unsigned) v) < 0) 
         goto loc_0x14002ba38
    loc_0x14002ba38: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002ba2b(x)
         v = ebx - 0xd
         if (v) 
    loc_0x14002ba3d: // orphan
         eax = r15d
         cdq
         eax -= edx
         eax >>= 1
         r15d = eax
         ebx = rax + 1
         rcx = ebx
         fcn.1400474ec ()         // fcn.1400474ec(0x0)
         rdi = rax
         eax = 0
         qword [var_38h] = rax
         qword [var_30h] = rax
         dword [var_sp_28h] = ebx
         qword [var_sp_20h] = rdi
         r9d = rax - 1
         r8 = r13                 // LPCWCH lpWideCharStr
         edx = 0                  // DWORD dwFlags
         ecx = 0x4e4              // 1252 // UINT CodePage // (pstr 0xffffffff) "\n"
         qword [sym.imp.KERNEL32.dll_WideCharToMultiByte] () // [0x14006a240:8]=0x91af0 reloc.KERNEL32.dll_WideCharToMultiByte // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_WideCharToMultiByte // int WideCharToMultiByte(-1, 0, 0, 0x0a0a000a, -1, -1, -1, -1)
         edx = dword [var_44h]
         
         goto loc_0x14002ba85
    loc_0x14002ba85: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002ba36(x), 0x14002ba3b(x)
         edx = dword [var_44h]
         
         goto loc_0x14002ba8b
    loc_0x14002ba8b: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002b97c(x)
         r15 = qword [var_bp_58h]

    loc_0x14002ba8f: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002ba89(x)
         rdi = r13

    loc_0x14002ba92: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002ba83(x)
         v = rdi & rdi
         if (!v) 
    loc_0x14002ba9b: // orphan
         v = r15d & r15d
         if (v <= 0) 
    loc_0x14002baa4: // orphan
         r8d = r15d               // int64_t arg3
         ecx = r12d               // int64_t arg1
         fcn.14002d080 ()         // fcn.14002d080(0x0, 0x0, 0x0)
         eax += 0xc
         dword [var_4ch] = eax
         rdx = eax                // SIZE_T dwBytes
         ecx = 2                  // UINT uFlags // reloc.OLEAUT32.dll_SysReAllocString
         qword [sym.imp.KERNEL32.dll_GlobalAlloc] () // [0x14006a550:8]=0x917c4 reloc.KERNEL32.dll_GlobalAlloc // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalAlloc // HGLOBAL GlobalAlloc(0x00000000, 12)
         r15 = rax
         v = rax & rax
         if (v) 
         goto loc_0x14002bacc
    loc_0x14002bacc: // orphan
         v = rsi & rsi
         if (v) 
    loc_0x14002bad1: // orphan
         v = dword [arg_80h] - esi
         if (!v) 
    loc_0x14002badd: // orphan
         rax = rip + 0x4964c      // u"The" // 0x140075130
         rdx = rip + 0x495f5      // 0x1400750e0
         v = dword [lpText] - 1
         if (!zf) rdx = rax       // u"The"
         r9d = r15 + 0x30
         r8 = rip + 0x49669       // u"EVKey Converter" // 0x140075168
         
         goto loc_0x14002bb04
    loc_0x14002bb04: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002baca(x), 0x14002bacf(x)
         rcx = r15                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         rbx = rax
         v = rax & rax
         if (!v) 
    loc_0x14002bb19: // orphan
         dword [var_60h] = 0xffffffff // -1
         byte [var_30h] = 1       // int64_t arg_110h
         rax = var_4ch
         qword [var_sp_28h] = rax // int64_t arg_108h
         rax = var_60h            // (pstr 0xffffffff) "\n"
         qword [var_sp_20h] = rax // int64_t arg_100h
         r9 = rbx                 // int64_t arg4
         r8 = rdi                 // int64_t arg3
         edx = dword [var_44h]    // int64_t arg2
         ecx = r12d               // int64_t arg1
         fcn.140045c30 ()         // fcn.140045c30(0x0, 0x0, 0xc, 0x0, 0x0, 0x0, 0x0)
         rcx = qword [var_48h]
         dword [rcx] = eax
         v = eax & eax
         if (!v) 
         goto loc_0x14002bb54
    loc_0x14002bb54: // orphan
         v = rsi & rsi
         if (v) 
    loc_0x14002bb59: // orphan
         v = dword [arg_80h] - esi
         if (!v) 
    loc_0x14002bb65: // orphan
         rax = rip + 0x4961c      // u"There was converting error" // 0x140075188
         rdx = rip + 0x4964d      // 0x1400751c0
         v = dword [lpText] - 1
         if (!zf) rdx = rax       // u"There was converting error"
         r9d = rsi + 0x30
         r8 = rip + 0x49689       // u"EVKey Converter" // 0x140075210
         
    loc_0x14002bb8c: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002bb52(x), 0x14002bb57(x)
         eax = 0xf1e3
         word [var_bp_78h] = ax
         r12d = 0
         word [var_80h] = r12w
         eax = dword [var_44h]
         dword [var_7ch] = eax
         rcx = dword [var_4ch]
         xmm0 = qword [var_80h]
         qword [rcx + rbx] = xmm0
         eax = dword [var_bp_78h]
         dword [rcx + rbx + 8] = eax
         dword [var_4ch] += 0xc   // [0xc:4]=0
         v = rdi - r13
         if (!v) 
         goto loc_0x14002bbc4
    loc_0x14002bbc4: // orphan
         rcx = rdi
         fcn.1400474e4 ()

    loc_0x14002bbcc: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bbc2(x)
         rax = r12
         v = rdi - r13
         if (!v) rax = rdi
         rdi = rax
         rcx = r14                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)
         r13 = r12
         rcx = r15                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)
         rdx = dword [var_4ch]    // SIZE_T dwBytes
         r8d = 2                  // UINT uFlags // reloc.OLEAUT32.dll_SysReAllocString
         rcx = r15                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalReAlloc] () // [0x14006a228:8]=0x91ac8 reloc.KERNEL32.dll_GlobalReAlloc // reloc.KERNEL32.dll_GlobalReAlloc // HGLOBAL GlobalReAlloc(0, 12, 0x00000000)
         rbx = rax
         qword [sym.imp.USER32.dll_EmptyClipboard] () // [0x14006a898:8]=0x91d82 reloc.USER32.dll_EmptyClipboard // reloc.USER32.dll_EmptyClipboard // BOOL EmptyClipboard(void)
         v = rsi & rsi
         if (!v) 
         goto loc_0x14002bc10
    loc_0x14002bc10: // orphan
         v = byte [var_68h] - r12b
         if (!v) 
         goto loc_0x14002bc17
    loc_0x14002bc17: // orphan
         fcn.14002be00 ()
         
         goto loc_0x14002bc1e
    loc_0x14002bc1e: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bc15(x)
         fcn.14002be70 ()

    loc_0x14002bc23: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bc1c(x)
         rdx = rsi                // HANDLE hMem
         ecx = eax                // UINT uFormat
         qword [sym.imp.USER32.dll_SetClipboardData] () // [0x14006a878:8]=0x91d24 reloc.USER32.dll_SetClipboardData // "$\x1d\t" // reloc.USER32.dll_SetClipboardData // HANDLE SetClipboardData(0, 0)

    loc_0x14002bc2e: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bc0e(x)
         rdx = rbx                // HANDLE hMem
         r12d = dword [uFormat]
         ecx = r12d               // UINT uFormat
         qword [sym.imp.USER32.dll_SetClipboardData] () // [0x14006a878:8]=0x91d24 reloc.USER32.dll_SetClipboardData // "$\x1d\t" // reloc.USER32.dll_SetClipboardData // HANDLE SetClipboardData(0, 0)
         r15 = rax
         qword [sym.imp.KERNEL32.dll_GetLastError] () // [0x14006a4a0:8]=0x91942 reloc.KERNEL32.dll_GetLastError // "B\x19\t" // reloc.KERNEL32.dll_GetLastError // DWORD GetLastError(void)
         v = byte [var_sp_40h] - r13b
         if (!v) 
         goto loc_0x14002bc4f
    loc_0x14002bc4f: // orphan
         rcx = rip + 0x48ba6      // "###" // 0x1400747fc // LPCSTR lpString
         qword [sym.imp.KERNEL32.dll_lstrlenA] () // [0x14006a238:8]=0x91ae4 reloc.KERNEL32.dll_lstrlenA // reloc.KERNEL32.dll_lstrlenA // int lstrlenA(0x6c206f5400232323)
         r8d = rax + 2            // reloc.OLEAUT32.dll_SysReAllocString
         rdx = r8d                // SIZE_T dwBytes // reloc.OLEAUT32.dll_SysReAllocString
         ecx = 2                  // UINT uFlags // reloc.OLEAUT32.dll_SysReAllocString
         qword [sym.imp.KERNEL32.dll_GlobalAlloc] () // [0x14006a550:8]=0x917c4 reloc.KERNEL32.dll_GlobalAlloc // reloc.KERNEL32.dll_GlobalAlloc // HGLOBAL GlobalAlloc(0x00000000, 2)
         rbx = rax
         rcx = rax                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         rdx = rip + 0x48b7b      // "###" // 0x1400747fc // LPCSTR lpString2
         rcx = rax                // LPSTR lpString1
         qword [sym.imp.KERNEL32.dll_lstrcpyA] () // [0x14006a230:8]=0x91ad8 reloc.KERNEL32.dll_lstrcpyA // reloc.KERNEL32.dll_lstrcpyA // LPSTR lstrcpyA(0, 0x6c206f5400232323)
         rcx = rbx                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)
         ecx = 1
         
         goto loc_0x14002bc9a
    loc_0x14002bc9a: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bc4d(x)
         v = dword [var_44h] - 5
         if (v) 
         goto loc_0x14002bca1
    loc_0x14002bca1: // orphan
         edx = 4                  // SIZE_T dwBytes
         ecx = rdx - 2            // reloc.OLEAUT32.dll_SysReAllocString
         qword [sym.imp.KERNEL32.dll_GlobalAlloc] () // [0x14006a550:8]=0x917c4 reloc.KERNEL32.dll_GlobalAlloc // reloc.KERNEL32.dll_GlobalAlloc // HGLOBAL GlobalAlloc(0x00000000, 4)
         rbx = rax
         rcx = rax                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         v = rax & rax
         if (!v) 
         goto loc_0x14002bcc0
    loc_0x14002bcc0: // orphan
         dword [rax] = 0x42a      // [0x42a:4]=-1
         rcx = rbx                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)

    loc_0x14002bccf: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bcbe(x)
         ecx = 0x10

    loc_0x14002bcd4: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bc98(x)
         rdx = rbx                // HANDLE hMem
         qword [sym.imp.USER32.dll_SetClipboardData] () // [0x14006a878:8]=0x91d24 reloc.USER32.dll_SetClipboardData // "$\x1d\t" // reloc.USER32.dll_SetClipboardData // HANDLE SetClipboardData(0, 0)

    loc_0x14002bcdd: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bc9f(x)
         v = rsi & rsi
         if (v) 
         goto loc_0x14002bce2
    loc_0x14002bce2: // orphan
         v = r15 & r15
         if (!v) 
         goto loc_0x14002bce7
    loc_0x14002bce7: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bce0(x)
         dword [var_sp_48h] = 1
         dword [0x140095f2c] = r12d // [0x140095f2c:4]=1

    loc_0x14002bcf6: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bce5(x)
         qword [sym.imp.USER32.dll_CloseClipboard] () // [0x14006a870:8]=0x91d12 reloc.USER32.dll_CloseClipboard // reloc.USER32.dll_CloseClipboard // BOOL CloseClipboard(void)
         v = dword [arg_80h] - r13d
         if (!v) 
         goto loc_0x14002bd05
    loc_0x14002bd05: // orphan
         v = rsi & rsi
         if (v) 
         goto loc_0x14002bd0a
    loc_0x14002bd0a: // orphan
         v = r15 & r15
         if (v) 
    loc_0x14002bd0f: // orphan
         eax = 0
         ebx = eax
         rax = rip + str.There_was_converting_error // 0x140075280 // u"There was converting error"
         rdx = rip + 0x4950f      // 0x140075230
         v = dword [lpText] - 1
         if (!zf) rdx = rax       // LPCWSTR lpText // u"There was converting error" str.There_was_converting_error
         r9d = rsi + 0x30
         r8 = rip + 0x49583       // u"EVKey Converter" // 0x1400752b8 // LPCWSTR lpCaption
         rcx = qword [hWnd]       // HWND hWnd
         qword [sym.imp.USER32.dll_MessageBoxW] () // [0x14006aa48:8]=0x91f90 reloc.USER32.dll_MessageBoxW // reloc.USER32.dll_MessageBoxW // int MessageBoxW(0x6c894808245c8948, "T", "E", 0x00000000)
         
    loc_0x14002bd45: // orphan
         // CODE XREFS from fcn.14002b4e0 @ 0x14002bd08(x), 0x14002bd0d(x)
         r9d = 0x40               // '@'
         rcx = qword [hWnd]
         v = dword [lpText] - 1
         if (v) 
         goto loc_0x14002bd57
    loc_0x14002bd57: // orphan
         r8 = rip + 0x4957a       // u"EVKey Converter" // 0x1400752d8 // LPCWSTR lpCaption
         rdx = rip + 0x4959b      // u"Chuy\u1ec3n m\xe3 Clipboard th\xe0nh c\xf4ng." // 0x140075300 // LPCWSTR lpText
         qword [sym.imp.USER32.dll_MessageBoxW] () // [0x14006aa48:8]=0x91f90 reloc.USER32.dll_MessageBoxW // reloc.USER32.dll_MessageBoxW // int MessageBoxW(0, "C", "E", 0)

    loc_0x14002bd6b: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bd03(x)
         ebx = dword [var_sp_48h]
         
    loc_0x14002bd74: // orphan
         // CODE XREF from fcn.14002b4e0 @ 0x14002bd55(x)
         r8 = rip + 0x495c5       // u"EVKey Converter" // 0x140075340 // LPCWSTR lpCaption
         rdx = rip + str.Clipboard_has_just_converted_successfully. // 0x140075360 // u"Clipboard has just converted successfully." // LPCWSTR lpText
         qword [sym.imp.USER32.dll_MessageBoxW] () // [0x14006aa48:8]=0x91f90 reloc.USER32.dll_MessageBoxW // reloc.USER32.dll_MessageBoxW // int MessageBoxW(0, "C", "E", 0)
         ebx = dword [var_sp_48h]
         
}

```

## main_window_proc

```c
===== FUNCTION main_window_proc 0x140033450 =====
[{"addr":5368919120,"name":"fcn.140033450","size":2852,"is-pure":"false","realsz":2852,"noreturn":false,"recursive":false,"stackframe":160,"calltype":"ms","cost":951,"cc":248,"bits":64,"type":"fcn","nbbs":157,"tracecov":0,"is-lineal":true,"ninstrs":633,"edges":243,"ebbs":3,"signature":"fcn.140033450 (int64_t arg1, int64_t arg2, int64_t arg3, int64_t arg4);","minaddr":5368919120,"maxaddr":5368921972,"maxbbins":31,"midbbins":4.032,"ratbbins":7.689,"indegree":0,"outdegree":82,"nlocals":9,"nargs":4,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.140033450 (int64_t arg1, int64_t arg2, int64_t arg3, int64_t arg4) {
    loc_0x140033450:
        rax = rsp
        push (rsi)
        push (rdi)
        push (r14)
        rsp -= 0x60
        qword [rax - 0x38] = 0xfffffffffffffffe
        qword [rax + 0x18] = rbx
        qword [rax + 0x20] = rbp
        rbx = r9     // arg4
        r14 = r8     // arg3
        ebp = edx    // arg2
        rsi = rcx    // arg1
        edi = 0
        v = edx - dword [rcx + 0x48]
        if (v) goto loc_0x1400335cf // unlikely
        goto loc_0x140033481;
    loc_0x1400335cf:
        // CODE XREF from fcn.140033450 @ 0x14003347b(x)
        v = ebp - 0x31a // 794
        if (((unsigned) v) > 0) goto 0x140033ba5 // likely
        goto loc_0x1400335db;
    loc_0x140033ba5:
        // CODE XREF from fcn.140033450 @ 0x1400335d5(x)
        v = ebp - 0x401
        if (((unsigned) v) > 0) goto 0x140033cb7 // likely
        goto loc_0x140033bb1;
    loc_0x140033cb7:
        // CODE XREF from fcn.140033450 @ 0x140033bab(x)
        v = ebp - 0x8d2 // 2258
        if (((unsigned) v) > 0) goto case.0x140033cf3.1028 // case.default.0x140033617 // likely
        goto loc_0x140033cc3;
    loc_0x140033bbf:
        // XREFS: CODE 0x1400335f6  CODE 0x140033617  CODE 0x14003379d
        // XREFS: CODE 0x140033b92  CODE 0x140033cbd  CODE 0x140033cd2
        // XREFS: CODE 0x140033cf3
        v = ebp - dword [0x14009ca40] // [0x14009ca40:4]=0
        if (v) goto loc_0x140033bd8 // likely
        goto loc_0x140033bc7;
    loc_0x140033bd8:
        // CODE XREF from fcn.140033450 @ 0x140033bc5(x)
        r9 = rbx     // LPARAM lParam
        r8 = r14     // WPARAM wParam
        edx = ebp    // UINT Msg // rsp
        rcx = qword [rsi + 8] // HWND hWnd // reloc.OLEAUT32.dll_SysStringLen
        qword [sym.imp.USER32.dll_DefWindowProcW] () // [0x14006a6c0:8]=0x922e6 reloc.USER32.dll_DefWindowProcW // (pstr 0xffffffff) "\n" reloc.USER32.dll_DefWindowProcW // LRESULT DefWindowProcW(0x0000000000060000, 0x00000016, 0, 0)
        rdi = rax
        
    loc_0x140033bed:
        // XREFS(45)
        rax = rdi
        goto loc_0x140033cc9;
        goto loc_0x140033bb3;
        goto loc_0x1400335e1;
        goto loc_0x14003348b;
        return rax;
    loc_0x14003348b:
        rdx = rbx
        rcx = rax - 0x30
        fcn.140016440 () // fcn.140016440(0x0, 0x0)
        r9d = edi
        rbp = dword [0x14009c908] // [0x14009c908:4]=0
        r10d = ebp   // rsp
        r8 = rip + 0x37b0c // 0x14006afb8
        rbx = qword [var_48h]
        rsi = qword [0x14009c900] // [0x14009c900:8]=0
        v = ebp & ebp
        if (v <= 0) goto loc_0x140033522 // unlikely
        goto loc_0x1400334bc;
    loc_0x140033522:
        // CODE XREF from fcn.140033450 @ 0x1400334ba(x)
        v = r9d - ebp
        if (v >= 0) goto loc_0x140033569 // unlikely
        goto loc_0x140033527;
    loc_0x140033569:
        // CODE XREF from fcn.140033450 @ 0x140033525(x)
        rdx = rbp    // rsp
        rdx <<<= 4   // (pstr 0xffffffff) "\n"
        rdx += rsi   // (pstr 0xffffffff) "\n"
        
    loc_0x140033573:
        // CODE XREF from fcn.140033450 @ 0x140033567(x)
        rax = rbp    // rsp
        rax <<<= 4   // (pstr 0xffffffff) "\n"
        rax += rsi   // (pstr 0xffffffff) "\n"
        v = rdx - rax
        if (!v) goto loc_0x140033596 // unlikely
        goto loc_0x140033582;
        return rax;
    loc_0x1400334bc: // orphan
         

    loc_0x1400334c0: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033520(x)
         eax = r10d
         eax -= r9d
         jns 0x1400334ca          // likely

         goto loc_0x1400334c8
    loc_0x1400334c8: // orphan
         eax++

    loc_0x1400334ca: // orphan
         // CODE XREF from fcn.140033450 @ 0x1400334c6(x)
         eax >>= 1
         r11d = rax + r9
         rax = r11d
         rax += rax
         rdx = r8
         v = rbx & rbx
         if (!zf) rdx = rbx
         rax = qword [rsi + rax*8] // reloc.OLEAUT32.dll_SysStringLen
         rcx = r8
         v = rax & rax
         if (!zf) rcx = rax       // reloc.OLEAUT32.dll_SysStringLen
         rdx -= rcx

    loc_0x1400334f1: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033501(x)
         eax = word [rcx]         // reloc.OLEAUT32.dll_SysStringLen
         v = ax - word [rcx + rdx]
         if (v) 
         goto loc_0x1400334fa
    loc_0x1400334fa: // orphan
         rcx += 2                 // reloc.OLEAUT32.dll_SysReAllocString
         v = ax & ax
         if (v) 
         goto loc_0x140033503
    loc_0x140033503: // orphan
         ecx = edi
         
         goto loc_0x140033507
    loc_0x140033507: // orphan
         // CODE XREF from fcn.140033450 @ 0x1400334f8(x)
         ecx = ecx - ecx
         ecx |= 1

    loc_0x14003350c: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033505(x)
         v = ecx & ecx
         cmovns r10d r11d
         eax = r11 + 1
         cmovns eax r9d
         r9d = eax
         v = eax - r10d
         jl 0x1400334c0           // unlikely

    loc_0x140033527: // orphan
         rdx = r9d
         rdx <<<= 4
         rdx += rsi
         rax = qword [rdx]        // reloc.OLEAUT32.dll_SysStringLen
         r9 = r8
         v = rax & rax
         if (!zf) r9 = rax        // reloc.OLEAUT32.dll_SysStringLen
         v = rbx & rbx
         if (!zf) r8 = rbx
         r9 -= r8                 // reloc.OLEAUT32.dll_SysStringLen

    loc_0x140033550: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033563(x)
         ecx = word [r8]          // reloc.OLEAUT32.dll_SysStringLen
         eax = word [r8 + r9]     // reloc.OLEAUT32.dll_SysStringLen
         ecx -= eax
         if (v) 
         goto loc_0x14003355d
    loc_0x14003355d: // orphan
         r8 += 2                  // reloc.OLEAUT32.dll_SysReAllocString
         v = eax & eax
         if (v) 
         goto loc_0x140033565
    loc_0x140033565: // orphan
         // CODE XREF from fcn.140033450 @ 0x14003355b(x)
         v = ecx & ecx
         jns 0x140033573          // likely

    loc_0x140033573: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033567(x)
         rax = rbp                // rsp
         rax <<<= 4               // (pstr 0xffffffff) "\n"
         rax += rsi               // (pstr 0xffffffff) "\n"
         v = rdx - rax
         if (!v) 
         goto loc_0x140033582
    loc_0x140033582: // orphan
         fcn.1400329a0 ()
         fcn.1400325d0 ()
         fcn.1400325e0 ()
         rbx = qword [var_48h]

    loc_0x140033596: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033580(x)
         rcx = rbx
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         eax = 0
         
         goto loc_0x1400335a5
    loc_0x1400335a5: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033485(x)
         rax = r8 - 4
         v = rax & 0xffffffffffff7fff
         if (v) 
         goto loc_0x1400335b1
    loc_0x1400335b1: // orphan
         rcx = qword [0x14009c918] // [0x14009c918:8]=0
         v = rcx & rcx
         if (!v) 
         goto loc_0x1400335bd
    loc_0x1400335bd: // orphan
         qword [sym.imp.KERNEL32.dll_SetEvent] () // [0x14006a520:8]=0x9181c reloc.KERNEL32.dll_SetEvent // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_SetEvent // BOOL SetEvent(0)

    loc_0x1400335c3: // orphan
         // CODE XREF from fcn.140033450 @ 0x1400335bb(x)
         fcn.1400329a0 ()

    loc_0x1400335c8: // orphan
         // CODE XREF from fcn.140033450 @ 0x1400335af(x)
         eax = 0
         
    loc_0x1400335db: // orphan
         if (!v) 
         goto loc_0x1400335e1
    loc_0x1400335e1: // orphan
         v = ebp - 0x7e           // '~'
         if (((unsigned) v) > 0) 
         goto loc_0x1400335ea
    loc_0x1400335ea: // orphan
         if (!v) 
    loc_0x1400335f0: // orphan
         eax = rdx - 1
         v = eax - 0x2b           // '+'
         if (((unsigned) v) > 0) 
         goto loc_0x1400335fc
    loc_0x1400335fc: // orphan
         r10 = rip - 0x33603      // 0x140000000
         eax = byte [r10 + rax + 0x33f90]
         ecx = dword [r10 + rax*4 + 0x33f74] // (pstr 0xffffffff) "\n"
         rcx += r10               // case.0x140033617.1
         
        return rax;
    loc_0x140033619: // case 1
         // CODE XREF from fcn.140033450 @ 0x140033617(x)
         rcx = rip + str.TaskbarCreated // 0x140075b90 // u"TaskbarCreated" // LPCWSTR lpString
         qword [sym.imp.USER32.dll_RegisterWindowMessageW] () // [0x14006a7e8:8]=0x92440 reloc.USER32.dll_RegisterWindowMessageW // "@$\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_RegisterWindowMessageW // UINT RegisterWindowMessageW("T")
         dword [0x14009ca40] = eax // [0x14009ca40:4]=0
         r9d = 0                  // TIMERPROC lpTimerFunc
         ebx = 1
         r8d = ebx                // UINT uElapse
         edx = 0x4ebc             // UINT_PTR nIDEvent // (pstr 0xffffffff) "\n"
         rcx = qword [0x140096148] // [0x140096148:8]=0 // HWND hWnd
         qword [sym.imp.USER32.dll_SetTimer] () // [0x14006a978:8]=0x92134 reloc.USER32.dll_SetTimer // "4!\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_SetTimer // UINT_PTR SetTimer(0, -1, 0x00000000, 0)
         r9d = 0                  // TIMERPROC lpTimerFunc
         edx = 0x4ebd             // UINT_PTR nIDEvent // (pstr 0xffffffff) "\n"
         r8d = 0xc8               // UINT uElapse
         rcx = qword [0x140096148] // [0x140096148:8]=0 // HWND hWnd
         qword [sym.imp.USER32.dll_SetTimer] () // [0x14006a978:8]=0x92134 reloc.USER32.dll_SetTimer // "4!\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_SetTimer // UINT_PTR SetTimer(0, -1, 0x00000000, 0)
         r9d = 0                  // TIMERPROC lpTimerFunc
         edx = 0x4ebe             // UINT_PTR nIDEvent // (pstr 0xffffffff) "\n"
         r8d = 0x190              // 400 // UINT uElapse // (pstr 0xffffffff) "\n"
         rcx = qword [0x140096148] // [0x140096148:8]=0 // HWND hWnd
         qword [sym.imp.USER32.dll_SetTimer] () // [0x14006a978:8]=0x92134 reloc.USER32.dll_SetTimer // "4!\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_SetTimer // UINT_PTR SetTimer(0, -1, -1, 0)
         r9d = 0                  // TIMERPROC lpTimerFunc
         edx = 0x4ebf             // UINT_PTR nIDEvent // (pstr 0xffffffff) "\n"
         r8d = 0x258              // 600 // UINT uElapse // (pstr 0xffffffff) "\n"
         rcx = qword [0x140096148] // [0x140096148:8]=0 // HWND hWnd
         qword [sym.imp.USER32.dll_SetTimer] () // [0x14006a978:8]=0x92134 reloc.USER32.dll_SetTimer // "4!\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_SetTimer // UINT_PTR SetTimer(0, -1, -1, 0)
         r9d = 0                  // TIMERPROC lpTimerFunc
         edx = 0x4ec0             // UINT_PTR nIDEvent // (pstr 0xffffffff) "\n"
         r8d = 0x320              // 800 // UINT uElapse // (pstr 0xffffffff) "\n"
         rcx = qword [0x140096148] // [0x140096148:8]=0 // HWND hWnd
         qword [sym.imp.USER32.dll_SetTimer] () // [0x14006a978:8]=0x92134 reloc.USER32.dll_SetTimer // "4!\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_SetTimer // UINT_PTR SetTimer(0, -1, -1, 0)
         edi = ebx
         
         goto loc_0x1400336bc
    loc_0x1400336bc: // case 'D'
         // CODE XREF from fcn.140033450 @ 0x140033617(x)
         rdx = rbx
         rcx = rip + 0x6925a      // 0x14009c920
         fcn.140031370 ()         // fcn.140031370(0x0, 0x0)
         edi = 1
         
    loc_0x1400336d5: // case 'C'
         // CODE XREF from fcn.140033450 @ 0x140033617(x)
         v = dword [r9] - 1
         if (v) 
         goto loc_0x1400336db
    loc_0x1400336db: // orphan
         rdx = rbx
         rcx = rip + 0x6923b      // 0x14009c920
         fcn.140030f90 ()         // fcn.140030f90(0x0, 0x0)

    loc_0x1400336ea: // orphan
         // XREFS: CODE 0x1400336d9  CODE 0x140033a9c  CODE 0x140033aa9   // XREFS: CODE 0x140033ac1  CODE 0x140033adb  CODE 0x140033aed   // XREFS: CODE 0x140033c1c  CODE 0x140033c7e  
         edi = 1
         
    loc_0x1400336f4: // case 17
         // CODE XREF from fcn.140033450 @ 0x140033617(x)
         rcx = rip + 0x69285      // 0x14009c980
         fcn.140036850 ()         // fcn.140036850(0x0)
         ebx = 1
         v = ebp - 2              // reloc.OLEAUT32.dll_SysReAllocString
         if (v) 
         goto loc_0x14003370a
    loc_0x14003370a: // orphan
         ecx = ebx                // int nExitCode
         qword [sym.imp.USER32.dll_PostQuitMessage] () // [0x14006a7c8:8]=0x92490 reloc.USER32.dll_PostQuitMessage // (pstr 0xffffffff) "\n" reloc.USER32.dll_PostQuitMessage // VOID PostQuitMessage(0)

    loc_0x140033712: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033708(x)
         rdi = rbx
         
    loc_0x14003371a: // case '&'
         // CODE XREF from fcn.140033450 @ 0x140033617(x)
         v = rbx & rbx
         if (!v) 
    loc_0x140033723: // orphan
         rdx = rip + str.ImmersiveColorSet // 0x140075bb0 // u"ImmersiveColorSet"
         rcx = rdi

    loc_0x140033730: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033758(x)
         eax = word [r9 + rcx*2]  // reloc.OLEAUT32.dll_SysStringLen
         v = ax - word [rdx + rcx*2]
         if (v) 
    loc_0x14003373f: // orphan
         eax = word [r9 + rcx*2 + 2]
         v = ax - word [rdx + rcx*2 + 2]
         if (v) 
    loc_0x140033750: // orphan
         rcx += 2                 // reloc.OLEAUT32.dll_SysReAllocString
         v = rcx - 0x12
         if (v) 
         goto loc_0x14003375a
    loc_0x14003375a: // case '!'
         // CODE XREFS from fcn.140033450 @ 0x1400335db(x), 0x1400335ea(x), 0x140033617(x), 0x140033bb9(x)
         rdx = rip + 0x6918f      // 0x14009c8f0
         rcx = qword [rsi + 8]
         fcn.140030740 ()         // fcn.140030740(0x0, 0x0)
         cl = 1
         fcn.140016ec0 ()
         
    loc_0x140033776: // orphan
         // CODE XREF from fcn.140033450 @ 0x1400335e4(x)
         v = ebp - 0x2e0          // 736
         if (((unsigned) v) > 0) 
         goto loc_0x140033782
    loc_0x140033782: // orphan
         if (!v) 
         goto loc_0x140033788
    loc_0x140033788: // orphan
         eax = ebp                // rsp
         eax -= 0xff
         if (!v) 
         goto loc_0x140033795
    loc_0x140033795: // orphan
         eax -= 0x12
         if (!v) 
         goto loc_0x14003379a
    loc_0x14003379a: // orphan
         v = eax - 2              // reloc.OLEAUT32.dll_SysReAllocString
         if (v) 
    loc_0x1400337a3: // orphan
         rax = r8 - 0x4ebc
         v = rax - 4
         if (((unsigned) v) > 0) 
    loc_0x1400337b4: // orphan
         rdx = r14                // UINT_PTR uIDEvent
         rcx = qword [rcx + 8]    // HWND hWnd
         qword [sym.imp.USER32.dll_KillTimer] () // [0x14006a930:8]=0x92140 reloc.USER32.dll_KillTimer // "@!\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_KillTimer // BOOL KillTimer(0, 0)

    loc_0x1400337c1: // case 1027
         // CODE XREF from fcn.140033450 @ 0x140033cf3(x)
         rcx = rip + 0x691b8      // 0x14009c980
         fcn.140036830 ()         // fcn.140036830(0x0)
         v = al & al
         if (!v) 
    loc_0x1400337d5: // orphan
         fcn.1400409a0 ()
         v = dword [rax + 4] - edi
         if (!v) 
    loc_0x1400337e3: // orphan
         cl = 1
         fcn.140034100 ()
         
    loc_0x1400337ef: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033798(x)
         v = r14 - 0xc787
         if (((unsigned) v) > 0) 
         goto loc_0x1400337fc
    loc_0x1400337fc: // orphan
         if (!v) 
         goto loc_0x140033802
    loc_0x140033802: // orphan
         rax = r14
         rax -= 0x72
         if (!v) 
         goto loc_0x14003380f
    loc_0x14003380f: // orphan
         rax -= 0x376             // 886
         if (!v) 
         goto loc_0x14003381b
    loc_0x14003381b: // orphan
         rax -= 0xbf6a
         if (!v) 
         goto loc_0x140033827
    loc_0x140033827: // orphan
         rax -= 0x11
         if (!v) 
         goto loc_0x140033831
    loc_0x140033831: // orphan
         v = rax - 1
         if (v) 
         goto loc_0x14003383b
    loc_0x14003383b: // orphan
         // CODE XREF from fcn.140033450 @ 0x14003392e(x)
         v = byte [0x14009c9bc] - dil // [0x14009c9bc:1]=0
         dil = v != 0
         eax = dword [0x14009c9c4] // [0x14009c9c4:4]=0
         rdx = rax * 0x2c
         r10 = rip - 0x33857      // 0x140000000
         eax = dword [0x14009c9c0] // [0x14009c9c0:4]=0
         rcx = rax * 0x2c
         rax = qword [rsi + 8]
         qword [var_28h] = rax
         ebx = 1
         dword [cbSizeHeader] = ebx
         r9d = edi
         r8 = var_80h
         edx = dword [rdx + r10 + 0x95c68]
         ecx = dword [rcx + r10 + 0x95978]
         fcn.14002b4e0 ()         // fcn.14002b4e0(0x0, 0x0, 0x14, 0x14, 0x178080, 0x0)
         edi = ebx
         
    loc_0x14003389a: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033809(x)
         v = word [0x14009c980] - di // [0x14009c980:2]=0
         if (!v) 
    loc_0x1400338a7: // orphan
         rdx = rip + 0x690d2      // 0x14009c980
         fcn.140034f30 ()         // fcn.140034f30(0x0)
         rcx = rip + 0x69066      // 0x14009c920
         fcn.14002fd00 ()         // fcn.14002fd00(0x0)
         rcx = rip + 0x690ba      // 0x14009c980
         fcn.140034ee0 ()         // fcn.140034ee0(0x0)
         word [0x14009c980] = di  // [0x14009c980:2]=0
         
    loc_0x1400338d7: // orphan
         // CODE XREF from fcn.140033450 @ 0x1400337fc(x)
         v = word [0x14009c980] - di // [0x14009c980:2]=0
         if (!v) 
         goto loc_0x1400338e0
    loc_0x1400338e0: // orphan
         rdx = rip + 0x69099      // 0x14009c980
         fcn.140034f30 ()         // fcn.140034f30(0x0)
         rcx = rip + 0x6902d      // 0x14009c920
         fcn.14002fd00 ()         // fcn.14002fd00(0x0)
         rcx = rip + 0x69081      // 0x14009c980
         fcn.140034ee0 ()         // fcn.140034ee0(0x0)
         word [0x14009c980] = di  // [0x14009c980:2]=0

    loc_0x14003390b: // orphan
         // CODE XREF from fcn.140033450 @ 0x1400338de(x)
         rcx = rsi + 0x38
         edx = 0
         fcn.140032dc0 ()         // fcn.140032dc0(0x0)
         
    loc_0x14003391b: // orphan
         // CODE XREF from fcn.140033450 @ 0x1400337f6(x)
         rax = r14
         rax -= 0xc790
         if (!v) 
         goto loc_0x14003392a
    loc_0x14003392a: // orphan
         rax -= 9
         if (!v) 
         goto loc_0x140033934
    loc_0x140033934: // orphan
         rax -= 3
         if (!v) 
         goto loc_0x14003393e
    loc_0x14003393e: // orphan
         rax -= 2
         if (!v) 
         goto loc_0x140033944
    loc_0x140033944: // orphan
         v = rax - 7
         if (!v) 
         goto loc_0x14003394a
    loc_0x14003394a: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033835(x)
         edx = r14d
         rcx = rip + 0x68fcc      // 0x14009c920
         fcn.14002f930 ()         // fcn.14002f930(0x0, 0x0)
         v = eax & eax
         if (!v) 
    loc_0x140033961: // orphan
         rdx = rip + 0x69018      // 0x14009c980
         rcx = rsi
         fcn.140034f30 ()         // fcn.140034f30(0x0)
         rcx = rip + 0x69009      // 0x14009c980
         fcn.140036850 ()         // fcn.140036850(0x0)
         rcx = qword [rsi + 0x38]
         v = rcx & rcx
         if (!v) 
         goto loc_0x140033985
    loc_0x140033985: // orphan
         rcx = qword [rcx]        // reloc.OLEAUT32.dll_SysStringLen
         v = rcx & rcx
         if (!v) 
    loc_0x14003398d: // orphan
         rdx = qword [rcx + 0x378]
         fcn.140028150 ()         // fcn.140028150(0x0, 0x0)

    loc_0x140033999: // orphan
         // CODE XREFS from fcn.140033450 @ 0x140033983(x), 0x14003398b(x)
         word [0x14009c980] = di  // [0x14009c980:2]=0
         
    loc_0x1400339a5: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033948(x)
         r8d = 0
         rdx = qword [rcx + 8]
         rcx = qword [rcx + 0x10]
         fcn.140018e30 ()         // fcn.140018e30(0x0, 0x0)
         
    loc_0x1400339ba: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033942(x)
         r8d = byte [0x14009c89b] // [0x14009c89b:1]=0
         ebx = 1
         r9d = ebx
         edx = 0
         ecx = ebx
         fcn.1400175f0 ()         // fcn.1400175f0(0x0, 0x0, 0x0, 0x1)
         
    loc_0x1400339d8: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033938(x)
         r8d = byte [0x14009c89b] // [0x14009c89b:1]=0
         ecx = byte [0x14009c9b9] // [0x14009c9b9:1]=0
         edx = 0
         r9d = rdx + 1
         fcn.1400175f0 ()         // fcn.1400175f0(0x0, 0x0, 0x0, 0x0)
         
    loc_0x1400339f7: // orphan
         // CODE XREF from fcn.140033450 @ 0x14003378f(x)
         ecx = dword [0x1400a5bb8] // [0x1400a5bb8:4]=0x1c000 // (pstr 0xffffffff) "\n"
         rax = qword gs:[0x58]
         r8d = 4
         rdx = qword [rax + rcx*8]
         eax = dword [r8 + rdx]
         v = dword [0x14009ca60] - eax // [0x14009ca60:4]=0
         if (v > 0) 
         goto loc_0x140033a20
    loc_0x140033a20: // orphan
         // CODE XREFS from fcn.140033450 @ 0x140033f34(x), 0x140033f6f(x)
         dword [cbSizeHeader] = 0x18 // [0x18:4]=0 // UINT cbSizeHeader
         r9 = pcbSize             // PUINT pcbSize
         r8d = 0                  // LPVOID pData
         edx = 0x10000003         // UINT uiCommand // (pstr 0xffffffff) "\n"
         rcx = rbx                // HRAWINPUT hRawInput
         qword [sym.imp.USER32.dll_GetRawInputData] () // [0x14006a6c8:8]=0x92540 reloc.USER32.dll_GetRawInputData // "@%\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetRawInputData // UINT GetRawInputData(0, -1, 0, 0x0000000000000c50, -1)
         edi = dword [pcbSize]
         v = qword [0x14009ca50] - rdi // [0x14009ca50:8]=0
         jae 0x140033a66          // unlikely

         goto loc_0x140033a51
    loc_0x140033a51: // orphan
         edx = edi
         rcx = rip + 0x68fee      // 0x14009ca48
         fcn.140004800 ()         // fcn.140004800(0x0, 0x0)
         qword [0x14009ca50] = rdi // [0x14009ca50:8]=0

    loc_0x140033a66: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033a4f(x)
         dword [cbSizeHeader] = 0x18 // [0x18:4]=0 // UINT cbSizeHeader
         r9 = pcbSize             // PUINT pcbSize
         r8 = qword [0x14009ca48] // [0x14009ca48:8]=0 // LPVOID pData
         edx = 0x10000003         // UINT uiCommand // (pstr 0xffffffff) "\n"
         rcx = rbx                // HRAWINPUT hRawInput
         qword [sym.imp.USER32.dll_GetRawInputData] () // [0x14006a6c8:8]=0x92540 reloc.USER32.dll_GetRawInputData // "@%\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetRawInputData // UINT GetRawInputData(0, -1, 0, 0x0000000000000c50, -1)
         rcx = qword [0x14009ca48] // [0x14009ca48:8]=0
         eax = dword [rcx]        // reloc.OLEAUT32.dll_SysStringLen
         v = eax & eax
         if (v) 
         goto loc_0x140033a98
    loc_0x140033a98: // orphan
         v = byte [rcx + 0x1c] & 1
         if (!v) 
    loc_0x140033aa2: // orphan
         fcn.140016d00 ()
         v = al & al
         if (v) 
    loc_0x140033aaf: // orphan
         fcn.1400329a0 ()
         edi = 1
         
    loc_0x140033abe: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033a96(x)
         v = eax - 1
         if (v) 
    loc_0x140033ac7: // orphan
         qword [sym.imp.KERNEL32.dll_GetTickCount] () // [0x14006a508:8]=0x9184c reloc.KERNEL32.dll_GetTickCount // "L\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GetTickCount // DWORD GetTickCount(void)
         dword [0x14009c89c] = eax // [0x14009c89c:4]=0
         ecx = dword [0x14009c8e0] // [0x14009c8e0:4]=1
         v = ecx & ecx
         if (!v) 
    loc_0x140033ae1: // orphan
         eax -= ecx
         cdq
         eax ^= edx
         eax -= edx
         v = eax - 0x1f4
         if (v <= 0) 
    loc_0x140033af3: // orphan
         fcn.1400329a0 ()
         fcn.1400325d0 ()
         fcn.1400325e0 ()
         edi = 1
         
    loc_0x140033b0c: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033782(x)
         r14 >>>= 0x10
         ecx = r14w
         fcn.140015570 ()         // fcn.140015570(0x0)
         fcn.140017450 ()
         cl = 1
         fcn.140016ec0 ()
         rdx = qword [rsi + 0x38]
         v = rdx & rdx
         if (!v) 
    loc_0x140033b32: // orphan
         rdx = qword [rdx]        // reloc.OLEAUT32.dll_SysStringLen
         v = rdx & rdx
         if (!v) 
    loc_0x140033b3e: // orphan
         rax = qword [rdx]        // reloc.OLEAUT32.dll_SysStringLen
         rcx = dword [rax + 4]
         rcx = qword [rcx + rdx + 8] // HWND hWnd
         qword [sym.imp.USER32.dll_IsWindow] () // [0x14006a970:8]=0x91f3a reloc.USER32.dll_IsWindow // ":\x1f\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_IsWindow // BOOL IsWindow(0)
         v = eax & eax
         if (!v) 
    loc_0x140033b58: // orphan
         rax = qword [rsi + 0x38]
         rdx = qword [rax]        // reloc.OLEAUT32.dll_SysStringLen
         rax = qword [rdx]
         rcx = dword [rax + 4]
         rcx = qword [rcx + rdx + 8] // HWND hWnd
         qword [sym.imp.USER32.dll_DestroyWindow] () // [0x14006a758:8]=0x921b2 reloc.USER32.dll_DestroyWindow // (pstr 0xffffffff) "\n" reloc.USER32.dll_DestroyWindow // BOOL DestroyWindow(0)
         edx = 0
         rcx = rsi + 0x38
         fcn.140032dc0 ()         // fcn.140032dc0(0x0)
         rcx = rsi
         fcn.140034ab0 ()         // fcn.140034ab0(0x0)
         
    loc_0x140033b86: // orphan
         // CODE XREF from fcn.140033450 @ 0x14003377c(x)
         eax = ebp                // rsp
         eax -= 0x305             // 773
         if (!v) 
         goto loc_0x140033b8f
    loc_0x140033b8f: // orphan
         v = eax - 1
         if (v) 
    loc_0x140033b94: // orphan
         fcn.14002c7d0 ()
         
    loc_0x140033b9b: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033b8d(x)
         ecx = r14d
         fcn.14002c7d0 ()
         
    loc_0x140033bb1: // orphan
         if (!v) 
         goto loc_0x140033bb3
    loc_0x140033bb3: // orphan
         v = ebp - 0x320          // 800
         if (!v) 
    loc_0x140033bc7: // orphan
         rax = qword [0x14009c8e8] // [0x14009c8e8:8]=0
         dword [rax + 8] = edi
         ecx = 0
         fcn.140016ec0 ()

    loc_0x140033bed: // orphan
         // XREFS(45)
         rax = rdi

    loc_0x140033bf0: // orphan
         // CODE XREFS from fcn.140033450 @ 0x1400335a0(x), 0x1400335ca(x)
         r11 = var_60h
         rbx = qword [r11 + 0x30] // (pstr 0xffffffff) "\n"
         rbp = qword [r11 + 0x38] // "\n"
         rsp = r11
         r14 = pop ()             // rsp
         rdi = pop ()
         rsi = pop ()
         return

        return rax;
    loc_0x140033c05: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033bb1(x)
         rbx -= 0x201
         if (!v) 
         goto loc_0x140033c12
    loc_0x140033c12: // orphan
         rbx -= 2
         if (!v) 
         goto loc_0x140033c18
    loc_0x140033c18: // orphan
         v = rbx - 2              // reloc.OLEAUT32.dll_SysReAllocString
         if (v) 
    loc_0x140033c22: // orphan
         rcx = lpPoint            // LPPOINT lpPoint
         qword [sym.imp.USER32.dll_GetCursorPos] () // [0x14006aa38:8]=0x91fac reloc.USER32.dll_GetCursorPos // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetCursorPos // BOOL GetCursorPos(0x0000000001400812)
         rcx = qword [rsi + 8]    // HWND hWnd
         qword [sym.imp.USER32.dll_SetForegroundWindow] () // [0x14006a7f8:8]=0x92418 reloc.USER32.dll_SetForegroundWindow // (pstr 0xffffffff) "\n" reloc.USER32.dll_SetForegroundWindow // BOOL SetForegroundWindow(0)
         r9d = dword [var_3ch]
         r8d = dword [lpPoint]    // (pstr 0xffffffff) "\n"
         edx = rbx + 0x26
         rcx = rip + 0x68cd5      // 0x14009c920
         fcn.140031430 ()         // fcn.140031430(0x0, 0x0, 0x26, 0x14009c920)
         r9d = 0                  // LPARAM lParam
         r8d = 0                  // WPARAM wParam
         edx = 0                  // UINT Msg
         rcx = qword [rsi + 8]    // HWND hWnd
         qword [sym.imp.USER32.dll_PostMessageW] () // [0x14006a8a0:8]=0x91d94 reloc.USER32.dll_PostMessageW // (pstr 0xffffffff) "\n" reloc.USER32.dll_PostMessageW // BOOL PostMessageW(0, 0, 0, 0)
         edi = rbx - 1
         
    loc_0x140033c67: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033c16(x)
         r8d = 0                  // UINT gmdiFlags
         edx = 0                  // UINT fByPos
         rcx = qword [0x14009c930] // [0x14009c930:8]=0 // HMENU hMenu
         qword [sym.imp.USER32.dll_GetMenuDefaultItem] () // [0x14006a7b8:8]=0x924b6 reloc.USER32.dll_GetMenuDefaultItem // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetMenuDefaultItem // UINT GetMenuDefaultItem(0, 0, 0)
         ebx = eax
         v = eax - 0xffffffff
         if (!v) 
    loc_0x140033c84: // orphan
         fcn.1400326c0 ()
         r8d = ebx                // WPARAM wParam
         r9d = 0                  // LPARAM lParam
         edx = 0x111              // UINT Msg
         rcx = qword [rsi + 8]    // HWND hWnd
         qword [sym.imp.USER32.dll_PostMessageW] () // [0x14006a8a0:8]=0x91d94 reloc.USER32.dll_PostMessageW // (pstr 0xffffffff) "\n" reloc.USER32.dll_PostMessageW // BOOL PostMessageW(0, 0x00000000, 0, 0)
         edi = 1
         
    loc_0x140033ca8: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033c0c(x)
         fcn.1400326c0 ()
         edi = 1
         
    loc_0x140033cc3: // orphan
         if (!v) 
         goto loc_0x140033cc9
    loc_0x140033cc9: // orphan
         eax = rdx - 0x402
         v = eax - 0x6f           // 'o'
         if (((unsigned) v) > 0) 
    loc_0x140033cd8: // orphan
         r10 = rip - 0x33cdf      // 0x140000000
         eax = byte [r10 + rax + 0x33fe0]
         ecx = dword [r10 + rax*4 + 0x33fbc] // (pstr 0xffffffff) "\n"
         rcx += r10               // case.0x140033cf3.1026
         
        return rax;
    loc_0x140033cf5: // case 1126
         // CODE XREF from fcn.140033450 @ 0x140033cf3(x)
         rcx = rsi

    loc_0x140033cf8: // orphan
         // CODE XREF from fcn.140033450 @ 0x14003382b(x)
         fcn.140034ce0 ()         // fcn.140034ce0(0x0)
         edi = 1
         
    loc_0x140033d07: // case 1127
         // CODE XREF from fcn.140033450 @ 0x140033cf3(x)
         rcx = rsi

    loc_0x140033d0a: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033821(x)
         fcn.140034ab0 ()         // fcn.140034ab0(0x0)
         edi = 1
         
    loc_0x140033d19: // case 1131
         // CODE XREF from fcn.140033450 @ 0x140033cf3(x)
         rcx = rsi

    loc_0x140033d1c: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033924(x)
         fcn.140034a70 ()         // fcn.140034a70(0x0)
         edi = 1
         
    loc_0x140033d2b: // case 1128
         // CODE XREF from fcn.140033450 @ 0x140033cf3(x)
         r9d = edi
         v = byte [0x14009c9bc] - dil // [0x14009c9bc:1]=0
         r9b = v != 0
         eax = dword [0x14009c9c4] // [0x14009c9c4:4]=0
         rdx = rax * 0x2c
         eax = dword [0x14009c9c0] // [0x14009c9c0:4]=0
         rcx = rax * 0x2c
         qword [var_28h] = rdi
         dword [cbSizeHeader] = edi
         r8 = var_30h
         edx = dword [rdx + r10 + 0x95c68] // "\n"
         ecx = dword [rcx + r10 + 0x95978] // "\n"
         fcn.14002b4e0 ()         // fcn.14002b4e0(0x0, 0x0, -1, -1, 0x178030, 0x0)
         edi = 1
         
    loc_0x140033d7a: // case 1137
         // CODE XREF from fcn.140033450 @ 0x140033cf3(x)
         v = ebp - 0x469
         if (v) 
         goto loc_0x140033d82
    loc_0x140033d82: // orphan
         ebx = 1
         word [0x14009c980] = bx  // [0x14009c980:2]=0
         dword [0x14009c988] = r14d // [0x14009c988:4]=0
         
         goto loc_0x140033d9a
    loc_0x140033d9a: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033d80(x)
         v = ebp - 0x471          // 1137
         if (v) 
         goto loc_0x140033da2
    loc_0x140033da2: // orphan
         eax = 8                  // reloc.OLEAUT32.dll_VariantClear
         word [0x14009c980] = ax  // [0x14009c980:2]=1
         eax = byte [0x14009ca28] // [0x14009ca28:1]=0
         v = al & 1
         if (!v) 
         goto loc_0x140033db9
    loc_0x140033db9: // orphan
         byte [0x14009ca28] = dil // [0x14009ca28:1]=0
         qword [0x14009c638] = rdi // [0x14009c638:8]=0x10000
         
    loc_0x140033dc9: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033db7(x)
         al |= 1
         byte [0x14009ca28] = al  // [0x14009ca28:1]=0
         qword [0x14009c638] = rdi // [0x14009c638:8]=0
         
    loc_0x140033dda: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033da0(x)
         v = ebp - 0x46d          // 1133
         if (v) 
         goto loc_0x140033de2
    loc_0x140033de2: // orphan
         v = byte [0x14009c9c9] - dil // [0x14009c9c9:1]=0
         byte [0x14009c9c9] = v == 0
         
         goto loc_0x140033df2
    loc_0x140033df2: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033de0(x)
         v = ebp - 0x470          // 1136
         if (v) 
         goto loc_0x140033dfa
    loc_0x140033dfa: // orphan
         v = byte [0x14009c9b8] - dil // [0x14009c9b8:1]=0
         byte [0x14009c9b8] = v == 0
         
    loc_0x140033e0a: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033df8(x)
         v = ebp - 0x46a
         if (v) 
    loc_0x140033e12: // orphan
         v = byte [0x14009c9d4] - dil // [0x14009c9d4:1]=0
         byte [0x14009c9d4] = v == 0

    loc_0x140033e20: // orphan
         // CODE XREFS from fcn.140033450 @ 0x140033df0(x), 0x140033e08(x)
         eax = 8                  // reloc.OLEAUT32.dll_VariantClear
         word [0x14009c980] = ax  // [0x14009c980:2]=8

    loc_0x140033e2c: // orphan
         // CODE XREFS from fcn.140033450 @ 0x140033d95(x), 0x140033dc7(x), 0x140033dd8(x), 0x140033e10(x)
         rdx = rip + 0x68b4d      // 0x14009c980
         rcx = rsi
         fcn.140034f30 ()         // fcn.140034f30(0x0)
         rcx = rip + 0x68ade      // 0x14009c920
         fcn.14002fd00 ()         // fcn.14002fd00(0x0)
         word [0x14009c980] = di  // [0x14009c980:2]=8
         rcx = rip + 0x68b2b      // 0x14009c980
         fcn.140036850 ()         // fcn.140036850(0x0)
         rcx = qword [rsi + 0x38]
         v = rcx & rcx
         if (!v) 
    loc_0x140033e67: // orphan
         rcx = qword [rcx]        // reloc.OLEAUT32.dll_SysStringLen
         v = rcx & rcx
         if (!v) 
    loc_0x140033e73: // orphan
         rdx = qword [rcx + 0x378]
         fcn.140028150 ()         // fcn.140028150(0x0, 0x0)
         
    loc_0x140033e84: // case 1026
         // CODE XREF from fcn.140033450 @ 0x140033cf3(x)
         v = r14 - 0x3e8          // 1000
         if (v) 
    loc_0x140033e91: // orphan
         rcx = rsi

    loc_0x140033e94: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033815(x)
         fcn.1400331a0 ()         // fcn.1400331a0(0x0)
         
    loc_0x140033e9e: // case 1124
         // CODE XREF from fcn.140033450 @ 0x140033cf3(x)
         v = r14 - 1
         if (v) 
    loc_0x140033ea8: // orphan
         v = rbx - r14
         if (v) 
    loc_0x140033eb1: // orphan
         rcx = qword [rsi + 0x40]
         v = rcx & rcx
         if (!v) 
         goto loc_0x140033eba
    loc_0x140033eba: // orphan
         eax = 0xffffffff         // -1 // "\n"
         lock xadd dword [rcx + 8] eax // "\n"
         v = eax - r14d
         if (v) 
    loc_0x140033ec9: // orphan
         rcx = qword [rsi + 0x40]
         rdx = qword [rcx]        // reloc.OLEAUT32.dll_SysStringLen
         v = rdx & rdx
         if (!v) 
         goto loc_0x140033ed5
    loc_0x140033ed5: // orphan
         rax = qword [rdx + 8]    // "\n"
         rcx = dword [rax + 4]
         rdx += 8                 // (pstr 0xffffffff) "\n" reloc.OLEAUT32.dll_VariantClear
         rcx += rdx               // u"xcept_app.\uff64\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff\uffff"
         rax = qword [rcx]
         rdx = r14
         qword [rax] ()           // 0xffffffffffffffff(0x0, 0x0, 0x0, 0xffffffffffff0009)
         rcx = qword [rsi + 0x40]

    loc_0x140033ef0: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033ed3(x)
         edx = 0x10
         fcn.1400474e4 ()

    loc_0x140033efa: // orphan
         // CODE XREFS from fcn.140033450 @ 0x140033eb8(x), 0x140033ec7(x)
         qword [rsi + 0x40] = rdi
         rcx = rip + 0x68a7b      // 0x14009c980
         fcn.140034ee0 ()         // fcn.140034ee0(0x0)
         
    loc_0x140033f0f: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033cc3(x)
         r8d = ebx
         edx = 0
         ecx = r14d
         fcn.140007f40 ()         // fcn.140007f40(0x0, 0x0, 0x0)
         
    loc_0x140033f21: // orphan
         // CODE XREF from fcn.140033450 @ 0x140033a1a(x)
         rcx = rip + 0x68b38      // 0x14009ca60
         fcn.1400479f4 ()         // fcn.1400479f4(0x0)
         v = dword [0x4009ca61] - 0xffffffff // [0x14009ca60:4]=0
         if (v) 
         goto loc_0x140033f3a
    loc_0x140033f3a: // orphan
         edx = 0x30               // '0'
         rcx = rip + 0x68b02      // 0x14009ca48
         fcn.140004800 ()         // fcn.140004800(0x0, 0x0)
         qword [0x14009ca50] = 0x30 // '0' // [0x14009ca50:8]=0
         rcx = rip + 0x352e3      // 0x140069240
         fcn.1400477c4 ()         // fcn.1400477c4(0x0)
         rcx = rip + 0x68af6      // 0x14009ca60
         fcn.140047994 ()         // fcn.140047994(0x0)
         
}

```

## low_level_keyboard_hook

```c
===== FUNCTION low_level_keyboard_hook 0x140031d70 =====
[{"addr":5368913264,"name":"fcn.140031d70","size":1690,"is-pure":"false","realsz":1690,"noreturn":false,"recursive":false,"stackframe":192,"calltype":"ms","cost":568,"cc":68,"bits":64,"type":"fcn","nbbs":91,"tracecov":0,"is-lineal":true,"ninstrs":388,"edges":143,"ebbs":2,"signature":"fcn.140031d70 (int64_t arg1, int64_t arg2, int64_t arg3, int64_t arg_a0h);","minaddr":5368913264,"maxaddr":5368914954,"maxbbins":28,"midbbins":4.264,"ratbbins":6.567,"indegree":0,"outdegree":50,"nlocals":14,"nargs":4,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.140031d70 (int64_t arg1, int64_t arg2, int64_t arg3, int64_t arg_a0h) {
    loc_0x140031d70:
        // DATA XREF from fcn.1400340b0 @ 0x1400340c7(r)
        push (rbp)
        push (rsi)
        push (rdi)
        push (r12)
        push (r13)
        push (r14)
        push (r15)
        rsp -= 0x60
        qword [var_48h] = 0xfffffffffffffffe
        qword [var_a0h] = rbx
        rdi = r8     // arg3
        rsi = rdx    // arg2
        ebp = ecx    // arg1
        v = byte [0x14009bf42] - 0 // [0x14009bf42:1]=0
        if (v) goto loc_0x140031dae // unlikely
        goto loc_0x140031da2;
    loc_0x140031dae:
        // CODE XREF from fcn.140031d70 @ 0x140031da0(x)
        ecx = 0
        fcn.140005b30 ()
        eax = eax
        v = qword [rdi + 0x10] - rax
        if (!v) goto loc_0x140031e6a // likely
        goto loc_0x140031dc1;
    loc_0x140031e6a:
        // CODE XREFS from fcn.140031d70 @ 0x140031dbb(x), 0x140031e47(x), 0x140031e56(x)
        qword [sym.imp.KERNEL32.dll_GetTickCount] () // [0x14006a508:8]=0x9184c reloc.KERNEL32.dll_GetTickCount // "L\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GetTickCount // DWORD GetTickCount(void)
        dword [0x14009c8e0] = eax // [0x14009c8e0:4]=1
        rax = rsi - 0x101
        v = rax & 0xfffffffffffffffb
        r14b = v != 0
        r13 = rdi + 4
        ecx = 0
        fcn.140005b30 ()
        eax = eax    // (pstr 0xffffffff) "\n"
        v = qword [rdi + 0x10] - rax
        if (!v) goto loc_0x140031ea5 // unlikely
        goto loc_0x140031e9a;
    loc_0x140031ea5:
        // CODE XREF from fcn.140031d70 @ 0x140031e98(x)
        rcx = qword [0x14009c860] // [0x14009c860:8]=0
        v = ebp & ebp
        js 0x140032195 // unlikely
        goto loc_0x140031eb4;
    loc_0x140032195:
        // XREFS: CODE 0x140031eae  CODE 0x140031eb7  CODE 0x140031ec6
        // XREFS: CODE 0x14003225d  CODE 0x14003226a  CODE 0x140032272
        edx = ebp    // rsp
        
    loc_0x140032197:
        // CODE XREFS from fcn.140031d70 @ 0x140032341(x), 0x140032362(x)
        r9 = rdi     // LPARAM lParam
        r8 = rsi     // WPARAM wParam
        rcx = qword [rcx + 0x848] // HHOOK hhk
        qword [sym.imp.USER32.dll_CallNextHookEx] () // [0x14006a7f0:8]=0x9242e reloc.USER32.dll_CallNextHookEx // ".$\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_CallNextHookEx // LRESULT CallNextHookEx(0, 0, 0, 0)
        goto loc_0x140031ebd;
        goto loc_0x140031dd0;
        return rax;
    loc_0x140031dc1: // orphan
         ecx = 0x11
         fcn.140005e40 ()         // fcn.140005e40(0x0)
         v = ax & ax
         js 0x140031df7           // unlikely

         goto loc_0x140031dd0
    loc_0x140031dd0: // orphan
         rax = qword [0x140096100] // [0x140096100:8]=0x40804009be00
         byte [rax + 0xa3] = 0
         rax = qword [0x140096100] // [0x140096100:8]=0x40804009be00
         byte [rax + 0xa2] = 0
         rax = qword [0x140096100] // [0x140096100:8]=0x40804009be00
         byte [rax + 0x11] = 0

    loc_0x140031df7: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140031dce(x)
         ecx = 0xa0               // int vKey // (pstr 0xffffffff) "\n"
         qword [sym.imp.USER32.dll_GetAsyncKeyState] () // [0x14006a8b8:8]=0x91db2 reloc.USER32.dll_GetAsyncKeyState // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetAsyncKeyState // SHORT GetAsyncKeyState(-1)
         ebx = ax
         ecx = 0xa1               // int vKey // (pstr 0xffffffff) "\n"
         qword [sym.imp.USER32.dll_GetAsyncKeyState] () // [0x14006a8b8:8]=0x91db2 reloc.USER32.dll_GetAsyncKeyState // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetAsyncKeyState // SHORT GetAsyncKeyState(-1)
         rcx = qword [0x140096100] // [0x140096100:8]=0x40804009be00
         v = bx & bx
         jns 0x140031e44          // likely

         goto loc_0x140031e1c
    loc_0x140031e1c: // orphan
         v = byte [rcx + 0xa0] - 0
         if (v >= 0) 
         goto loc_0x140031e25
    loc_0x140031e25: // orphan
         v = byte [rcx + 0x10] - 0
         jl 0x140031e44           // unlikely

    loc_0x140031e2b: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140031e23(x)
         byte [rcx + 0xa0] = 0x80 // [0x80:1]=0
         rcx = qword [0x140096100] // [0x140096100:8]=0x40804009be00
         byte [rcx + 0x10] = 0x80 // [0x80:1]=0
         rcx = qword [0x140096100] // [0x140096100:8]=0x40804009be00

    loc_0x140031e44: // orphan
         // CODE XREFS from fcn.140031d70 @ 0x140031e1a(x), 0x140031e29(x)
         v = ax & ax
         jns 0x140031e6a          // likely

         goto loc_0x140031e49
    loc_0x140031e49: // orphan
         v = byte [rcx + 0xa1] - 0
         if (v >= 0) 
         goto loc_0x140031e52
    loc_0x140031e52: // orphan
         v = byte [rcx + 0x10] - 0
         jl 0x140031e6a           // unlikely

    loc_0x140031e58: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140031e50(x)
         byte [rcx + 0xa1] = 0x80 // [0x80:1]=0
         rax = qword [0x140096100] // [0x140096100:8]=0x40804009be00
         byte [rax + 0x10] = 0x80 // [0x80:1]=0

    loc_0x140031e9a: // orphan
         edx = r14b
         ecx = dword [rdi]        // uint32_t arg1 // reloc.OLEAUT32.dll_SysStringLen
         fcn.140032620 ()         // fcn.140032620(0x0)

    loc_0x140031eb4: // orphan
         v = rcx & rcx
         if (!v) 
         goto loc_0x140031ebd
    loc_0x140031ebd: // orphan
         eax = dword [rdi]        // reloc.OLEAUT32.dll_SysStringLen
         eax--
         v = eax - 0xfd
         if (((unsigned) v) > 0) 
    loc_0x140031ecc: // orphan
         ecx = 0
         fcn.140005b30 ()
         eax = eax
         v = qword [rdi + 0x10] - rax
         if (!v) 
         goto loc_0x140031edb
    loc_0x140031edb: // orphan
         edx = r14b
         ecx = dword [rdi]        // reloc.OLEAUT32.dll_SysStringLen
         fcn.140014c80 ()         // fcn.140014c80(0x0)
         edx = r14b
         ecx = dword [rdi]        // uint32_t arg1 // reloc.OLEAUT32.dll_SysStringLen
         fcn.140032620 ()         // fcn.140032620(0x0)

    loc_0x140031ef1: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140031ed9(x)
         eax = dword [rdi]        // reloc.OLEAUT32.dll_SysStringLen
         v = eax - 0xe7
         if (!v) 
         goto loc_0x140031efe
    loc_0x140031efe: // orphan
         ecx = byte [0x14009c870] // [0x14009c870:1]=0
         eax = al
         v = r14b & r14b
         if (!zf) ecx = eax
         byte [0x14009c870] = cl  // [0x14009c870:1]=0
         fcn.140007ed0 ()
         v = al & al
         if (!v) 
         goto loc_0x140031f1d
    loc_0x140031f1d: // orphan
         v = byte [0x14009bf41] - 0 // [0x14009bf41:1]=0
         if (v) 
    loc_0x140031f2a: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140031f1b(x)
         r15d = 0
         v = qword [rdi + 0x10] - r15
         if (!v) 
         goto loc_0x140031f33
    loc_0x140031f33: // orphan
         bl = 0
         ecx = 0
         fcn.140005b30 ()
         eax = eax
         v = qword [rdi + 0x10] - rax
         if (!v) 
         goto loc_0x140031f44
    loc_0x140031f44: // orphan
         cl = 1
         fcn.140005b30 ()
         eax = dword [0x14009c874] // [0x14009c874:4]=0
         v = eax - 0x64           // 'd'
         if (v >= 0) 
         goto loc_0x140031f56
    loc_0x140031f56: // orphan
         eax++
         dword [0x14009c874] = eax // [0x14009c874:4]=0
         byte [0x14009610b] = 0x80 // [0x14009610b:1]=128
         fcn.140034050 ()
         byte [0x14009610b] = 0x80 // [0x14009610b:1]=128
         fcn.1400340b0 ()
         
    loc_0x140031f78: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140031f42(x)
         qword [rdi + 0x10] = r15
         bl = 1

    loc_0x140031f7e: // orphan
         // CODE XREFS from fcn.140031d70 @ 0x140031f54(x), 0x140031f76(x)
         fcn.140007ed0 ()
         v = al & al
         if (v) 
    loc_0x140031f8b: // orphan
         v = bl & bl
         if (!v) 
    loc_0x140031f8f: // orphan
         v = byte [0x14009bf41] - r15b // [0x14009bf41:1]=0
         if (v) 
    loc_0x140031f9c: // orphan
         // CODE XREFS from fcn.140031d70 @ 0x140031f31(x), 0x140031f8d(x)
         rdx = qword [0x140096100] // [0x140096100:8]=0x40804009be00
         eax = byte [rdx + 0xa3]
         al |= byte [rdx + 0xa2]
         al |= byte [rdx + 0x11]
         al &= 0x80
         byte [var_a8h] = al
         eax = byte [rdx + 0xa5]
         al |= byte [rdx + 0xa4]
         al |= byte [rdx + 0x12]
         al &= 0x80
         byte [var_a9h] = al
         r12d = byte [rdx + 0x14]
         r12b &= 1
         byte [var_aah] = r12b
         ebx = byte [rdx + 0xa1]
         bl |= byte [rdx + 0xa0]
         bl |= byte [rdx + 0x10]
         bl &= 0x80
         byte [var_abh] = bl
         ecx = byte [rdx + 0x5c]
         cl |= byte [rdx + 0x5b]
         cl &= 0x80
         eax = dword [var_a8h]    // (pstr 0xffffffff) "\n"
         dword [var_a8h] = eax
         byte [var_ach] = cl
         fcn.140006d10 ()
         v = byte [0x14009bf42] - r15b // [0x14009bf42:1]=0
         if (v) 
         goto loc_0x140032031
    loc_0x140032031: // orphan
         fcn.140007ed0 ()
         v = al & al
         if (!v) 
         goto loc_0x14003203a
    loc_0x14003203a: // orphan
         v = byte [0x14009bf41] - r15b // [0x14009bf41:1]=0
         if (v) 
    loc_0x140032047: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140032038(x)
         rdx = rdi                // int64_t arg2
         rcx = rsi                // int64_t arg1
         fcn.1400319a0 ()         // fcn.1400319a0(0x0, 0x0)
         v = eax - 1
         if (!v) 
    loc_0x14003205b: // orphan
         v = eax - 2              // reloc.OLEAUT32.dll_SysReAllocString
         if (!v) 
         goto loc_0x140032064
    loc_0x140032064: // orphan
         v = byte [0x14009bf42] - r15b // [0x14009bf42:1]=0
         if (v) 
         goto loc_0x140032071
    loc_0x140032071: // orphan
         rax = rsi - 0x101
         v = rax & 0xfffffffffffffffb
         bl = v == 0
         r9 = var_50h
         r8 = var_40h
         rdx = rdi
         rcx = rsi
         fcn.140032440 ()         // fcn.140032440(0x0, 0x0, 0x0)
         rax = qword [0x14009c860] // [0x14009c860:8]=0
         r12 = qword [rax + 0x858] // (pstr 0xffffffff) "\n"
         v = bl & bl
         if (v) 
    loc_0x1400320ac: // orphan
         rax = qword [0x14009c9a8] // [0x14009c9a8:8]=0
         v = qword [var_40h] - rax
         if (v) 
    loc_0x1400320be: // orphan
         ecx = dword [rdi]        // reloc.OLEAUT32.dll_SysStringLen
         ecx += 0xffffff90
         rdx = rip - 0x320ca      // 0x140000000
         v = ecx - 0xb
         if (((unsigned) v) > 0) 
    loc_0x1400320d3: // orphan
         v = byte [rcx + rdx + 0x9c9d7] - r15b
         if (!v) 
    loc_0x1400320e1: // orphan
         v = ecx - 0xb
         if (((unsigned) v) > 0) 
    loc_0x1400320ea: // orphan
         ecx = dword [rdx + rcx*4 + 0x3240c] // "\n"
         rcx += rdx               // "\n"
         
        return rax;
    loc_0x1400320f6: // case 0
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         cl = 1
         fcn.140032ad0 ()
         
    loc_0x140032102: // case 1
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         ecx = 0
         fcn.140032ad0 ()
         
    loc_0x14003210e: // case 2
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         ebx = 0x469
         
         goto loc_0x140032115
    loc_0x140032115: // case 3
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         ebx = 0x469
         r15d = dword [0x14009c98c] // [0x14009c98c:4]=0
         
    loc_0x140032123: // case 4
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         ebx = 0x467
         
         goto loc_0x14003212a
    loc_0x14003212a: // case 5
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         ebx = 0x466
         
    loc_0x140032131: // case 6
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         r9d = 0                  // LPARAM lParam
         r8d = 0                  // WPARAM wParam
         edx = 0x468              // UINT Msg
         rcx = r12                // HWND hWnd
         qword [sym.imp.USER32.dll_SendMessageW] () // [0x14006a968:8]=0x91f2a reloc.USER32.dll_SendMessageW // "*\x1f\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_SendMessageW // LRESULT SendMessageW(0, 0x00000000, 0, 0)
         
    loc_0x140032147: // case 7
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         ebx = 0x46b

    loc_0x14003214c: // orphan
         // CODE XREFS from fcn.140031d70 @ 0x140032128(x), 0x14003212f(x)
         rcx = r12                // HWND hWnd
         qword [sym.imp.USER32.dll_BringWindowToTop] () // [0x14006a808:8]=0x923f8 reloc.USER32.dll_BringWindowToTop // (pstr 0xffffffff) "\n" reloc.USER32.dll_BringWindowToTop // BOOL BringWindowToTop(0)
         rcx = r12                // HWND hWnd
         qword [sym.imp.USER32.dll_SetForegroundWindow] () // [0x14006a7f8:8]=0x92418 reloc.USER32.dll_SetForegroundWindow // (pstr 0xffffffff) "\n" reloc.USER32.dll_SetForegroundWindow // BOOL SetForegroundWindow(0)
         
         goto loc_0x140032160
    loc_0x140032160: // case 8
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         ebx = 0x46d              // 1133
         
    loc_0x140032167: // case 10
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         ebx = 0x471              // 1137

    loc_0x14003216c: // orphan
         // CODE XREFS from fcn.140031d70 @ 0x140032113(x), 0x140032121(x), 0x140032165(x)
         ecx = 0
         fcn.1400178a0 ()

    loc_0x140032173: // orphan
         // CODE XREF from fcn.140031d70 @ 0x14003215e(x)
         r9d = 0                  // LPARAM lParam
         r8 = r15                 // WPARAM wParam
         edx = ebx                // UINT Msg
         rcx = r12                // HWND hWnd
         qword [sym.imp.USER32.dll_PostMessageW] () // [0x14006a8a0:8]=0x91d94 reloc.USER32.dll_PostMessageW // (pstr 0xffffffff) "\n" reloc.USER32.dll_PostMessageW // BOOL PostMessageW(0, 0, 0, 0)
         dword [0x14009c87c] = 1  // [0x14009c87c:4]=0

    loc_0x14003218e: // orphan
         // XREFS: CODE 0x140031ef8  CODE 0x140031f24  CODE 0x140031f85   // XREFS: CODE 0x140031f96  CODE 0x140032055  CODE 0x1400320fd   // XREFS: CODE 0x140032109  CODE 0x140032145  CODE 0x14003221c  
         rcx = qword [0x14009c860] // [0x14009c860:8]=0

    loc_0x140032197: // orphan
         // CODE XREFS from fcn.140031d70 @ 0x140032341(x), 0x140032362(x)
         r9 = rdi                 // LPARAM lParam
         r8 = rsi                 // WPARAM wParam
         rcx = qword [rcx + 0x848] // HHOOK hhk
         qword [sym.imp.USER32.dll_CallNextHookEx] () // [0x14006a7f0:8]=0x9242e reloc.USER32.dll_CallNextHookEx // ".$\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_CallNextHookEx // LRESULT CallNextHookEx(0, 0, 0, 0)

    loc_0x1400321aa: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140032405(x)
         rbx = qword [var_a0h]
         rsp += 0x60
         r15 = pop ()             // rsp
         r14 = pop ()
         r13 = pop ()
         r12 = pop ()
         rdi = pop ()
         rsi = pop ()
         rbp = pop ()             // (pstr 0xffffffff) "\n" // (pstr 0xffffffff) "\n"
         return

        return rax;
    loc_0x1400321c2: // case 11
         // CODE XREF from fcn.140031d70 @ 0x1400320f4(x)
         fcn.1400329a0 ()
         byte [0x14009610b] = 0x80 // [0x14009610b:1]=128
         fcn.140034050 ()
         fcn.140006e70 ()
         v = qword [0x14009c860] - r15 // [0x14009c860:8]=0
         if (v) 
         goto loc_0x1400321e1
    loc_0x1400321e1: // orphan
         fcn.1400409a0 ()
         qword [0x14009c860] = rax // [0x14009c860:8]=0

    loc_0x1400321ed: // orphan
         // CODE XREF from fcn.140031d70 @ 0x1400321df(x)
         fcn.140005f50 ()
         ecx = 0
         fcn.140007d40 ()
         byte [0x14009610b] = 0x80 // [0x14009610b:1]=128
         fcn.1400340b0 ()

    loc_0x140032205: // case 0x1400320f4
         // XREFS: CODE 0x14003206b  CODE 0x1400320a6  CODE 0x1400320b8   // XREFS: CODE 0x1400320cd  CODE 0x1400320db  CODE 0x1400320e4   // XREFS: CODE 0x1400320f4  
         v = byte [0x14009c9ce] - r15b // [0x14009c9ce:1]=0
         if (!v) 
         goto loc_0x14003220e
    loc_0x14003220e: // orphan
         rax = qword [0x14009bf50] // [0x14009bf50:8]=0x10000
         v = qword [0x14009bf48] - rax // [0x14009bf48:8]=0
         if (v) 
    loc_0x140032222: // orphan
         // CODE XREF from fcn.140031d70 @ 0x14003220c(x)
         sub.USER32.dll_GetForegroundWindow () // HWND GetForegroundWindow(void)
         rcx = rax
         fcn.140008d90 ()         // fcn.140008d90(0x0)
         ecx = 0
         fcn.140040620 ()         // fcn.140040620(0x0)
         v = dword [rax + 8] - r15d
         if (!v) 
         goto loc_0x14003223c
    loc_0x14003223c: // orphan
         ecx = 0
         fcn.140040620 ()         // fcn.140040620(0x0)
         v = dword [rax + 0x10] - r15d
         if (!v) 
    loc_0x140032249: // orphan
         rcx = qword [0x14009c860] // [0x14009c860:8]=0
         
         goto loc_0x140032252
    loc_0x140032252: // orphan
         // CODE XREFS from fcn.140031d70 @ 0x14003223a(x), 0x140032247(x)
         rcx = qword [0x14009c860] // [0x14009c860:8]=0
         v = dword [rcx + 4] - r15d
         if (!v) 
    loc_0x140032263: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140032250(x)
         v = dword [0x140094003] - 1 // [0x140094004:4]=-1
         if (!v) 
    loc_0x140032270: // orphan
         v = ebp & ebp
         if (v) 
    loc_0x140032278: // orphan
         rax = var_a8h
         qword [var_30h] = rax
         qword [var_28h] = r13    // int64_t arg_a0h
         qword [var_20h] = rdi    // int64_t arg_98h
         r9d = r14b               // int64_t arg_90h
         r8 = rdi                 // int64_t arg3
         rdx = rdi                // int64_t arg2
         rcx = rsi                // int64_t arg1
         fcn.140032760 ()         // fcn.140032760(0x0, 0x0, 0x0, 0x0, 0x0, 0x0)
         v = eax - 1
         if (!v) 
    loc_0x1400322aa: // orphan
         ecx = dword [rdi]        // reloc.OLEAUT32.dll_SysStringLen
         eax = byte [0x14009c870] // [0x14009c870:1]=0
         al += 0x5e
         v = al - 1
         if (((unsigned) v) <= 0) 
         goto loc_0x1400322bd
    loc_0x1400322bd: // orphan
         eax = rcx - 0xa0
         v = eax - 1
         if (((unsigned) v) > 0) 
         goto loc_0x1400322cc
    loc_0x1400322cc: // orphan
         v = r14b & r14b
         if (v) 
    loc_0x1400322d5: // orphan
         v = byte [0x14009c402] - r15b // [0x14009c402:1]=0
         if (!v) 
    loc_0x1400322de: // orphan
         v = ecx - 0xa0
         if (v) 
         goto loc_0x1400322e6
    loc_0x1400322e6: // orphan
         byte [var_20h] = r15b
         edx = 0
         r9d = 2                  // reloc.OLEAUT32.dll_SysReAllocString
         r8d = r9d                // reloc.OLEAUT32.dll_SysReAllocString
         fcn.1400084c0 ()         // fcn.1400084c0(0x0, 0x0, 0x0, 0x0, 0x2)
         ecx = 0xa0
         
         goto loc_0x140032302
    loc_0x140032302: // orphan
         // CODE XREF from fcn.140031d70 @ 0x1400322e4(x)
         v = ecx - 0xa1
         if (v) 
    loc_0x14003230a: // orphan
         byte [var_20h] = r15b
         edx = 0
         r9d = 2                  // reloc.OLEAUT32.dll_SysReAllocString
         r8d = r9d                // reloc.OLEAUT32.dll_SysReAllocString
         fcn.1400084c0 ()         // fcn.1400084c0(0x0, 0x0, 0x0, 0x0, 0x2)
         ecx = 0xa1

    loc_0x140032324: // orphan
         // CODE XREF from fcn.140031d70 @ 0x140032300(x)
         edx = 0
         byte [var_20h] = r15b
         r9d = rdx + 2            // reloc.OLEAUT32.dll_SysReAllocString
         r8d = rdx + 1
         fcn.1400084c0 ()         // fcn.1400084c0(0x0, 0x0, 0x0, 0x0, 0x1)
         edx = 0
         rcx = qword [0x14009c860] // [0x14009c860:8]=0
         
         goto loc_0x140032346
    loc_0x140032346: // orphan
         // CODE XREF from fcn.140031d70 @ 0x1400322b7(x)
         v = r14b & r14b
         if (v) 
    loc_0x14003234b: // orphan
         fcn.140006450 ()         // fcn.140006450(0x0)
         v = al & al
         if (!v) 
    loc_0x140032354: // orphan
         fcn.1400329a0 ()

    loc_0x140032359: // orphan
         // XREFS: CODE 0x1400322c6  CODE 0x1400322cf  CODE 0x1400322dc   // XREFS: CODE 0x140032308  CODE 0x140032349  CODE 0x140032352  
         edx = 0
         rcx = qword [0x14009c860] // [0x14009c860:8]=0
         
    loc_0x140032367: // orphan
         // CODE XREFS from fcn.140031d70 @ 0x14003202b(x), 0x140032041(x)
         qword [var_b0h] = r15
         eax = byte [rdi + 8]
         al &= 1
         rcx = var_b0h
         qword [var_30h] = rcx
         byte [var_28h] = al
         byte [var_20h] = bl
         r9d = r12b
         r8d = r14b
         rdx = r13
         rcx = rdi
         fcn.140031680 ()         // fcn.140031680(0x0, 0x0, 0x0, 0x0, 0x0)
         rbx = qword [var_b0h]
         qword [var_b8h] = rbx
         v = rbx & rbx
         if (!v) 
         goto loc_0x1400323b2
    loc_0x1400323b2: // orphan
         lock inc dword [rbx + 8]
         rbx = qword [var_b0h]

    loc_0x1400323be: // orphan
         // CODE XREF from fcn.140031d70 @ 0x1400323b0(x)
         rcx = var_b8h
         fcn.140008320 ()         // fcn.140008320(0x0)
         v = rbx & rbx
         if (!v) 
    loc_0x1400323d1: // orphan
         eax = 0xffffffff         // -1 // "\n"
         lock xadd dword [rbx + 8] eax // "\n"
         v = eax - 1
         if (v) 
    loc_0x1400323e0: // orphan
         edx = rax + 0x13
         rbx = qword [var_b0h]
         rcx = qword [rbx]
         fcn.1400474e4 ()
         edx = 0x10
         rcx = rbx
         fcn.1400474e4 ()

    loc_0x140032400: // orphan
         // CODE XREFS from fcn.140031d70 @ 0x14003205e(x), 0x1400322a4(x), 0x1400323cf(x), 0x1400323de(x)
         eax = 1
         
}

```

## update_dispatch

```c
===== FUNCTION update_dispatch 0x140017480 =====
[{"addr":5368804480,"name":"fcn.140017480","size":355,"is-pure":"false","realsz":355,"noreturn":false,"recursive":false,"stackframe":160,"calltype":"ms","cost":124,"cc":5,"bits":64,"type":"fcn","nbbs":9,"tracecov":0,"is-lineal":true,"ninstrs":92,"edges":12,"ebbs":1,"signature":"fcn.140017480 ();","minaddr":5368804480,"maxaddr":5368804835,"maxbbins":42,"midbbins":10.222,"ratbbins":4.109,"indegree":2,"outdegree":12,"nlocals":8,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.140017480 (int rcx, int rdx) {
    loc_0x140017480:
        // CALL XREF from fcn.140033128 @ 0x1400247b4(x)
        // CALL XREF from fcn.140034f90 @ 0x140035464(x)
        rax = rsp
        push (rsi)
        push (rdi)
        push (r14)
        rsp -= 0x60
        qword [rax - 0x48] = 0xfffffffffffffffe
        qword [rax + 8] = rbx
        qword [rax + 0x10] = rbp
        ebp = dl
        r14d = cl
        rcx = rax - 0x30
        fcn.140016270 () // fcn.140016270(0x0)
        r8 = qword [rax]
        rsi = rip + 0x53b02 // 0x14006afb8
        rdx = rsi
        v = r8 & r8
        if (!zf) rdx = r8
        rcx = var_38h // (pstr 0xfffffffe) "!\n"
        fcn.140038400 () // fcn.140038400(0x0, 0x14006afb8)
        rcx = qword [var_48h]
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        edx = dword [var_40h] // "!\n"
        edx += 0xb
        rcx = var_38h
        fcn.140008a10 () // fcn.140008a10(0x0, 0x14006afb8)
        rcx = dword [var_40h]
        rax = qword [var_38h]
        rdx = rax + rcx*2
        xmm0 = xmmword [0x14006d5d8] // [0x14006d5d8:16]=-1
        xmmword [rdx] = xmm0
        eax = dword [0x14006d5e8] // [0x14006d5e8:4]=0x650078 // (pstr 0xffffffff) "\n"
        dword [rdx + 0x10] = eax
        eax = word [0x14006d5ec] // [0x14006d5ec:2]=0
        word [rdx + 0x14] = ax
        eax = dword [var_40h]
        eax += 0xa
        dword [var_40h] = eax
        edi = 0
        v = eax & eax
        if (v <= 0) goto loc_0x14001754d // unlikely
        goto loc_0x140017523;
    loc_0x14001754d:
        // CODE XREFS from fcn.140017480 @ 0x140017521(x), 0x140017528(x)
        rcx = qword [var_38h] // (pstr 0xffffffff) "\n"
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        v = rdi & rdi
        if (!zf) rsi = rdi
        edx = 0
        rcx = rsi
        fcn.140053f44 ()
        rcx = rdi
        v = eax - 0xffffffff
        if (!v) goto loc_0x1400175c9 // unlikely
        goto loc_0x140017570;
    loc_0x1400175c9:
        // CODE XREF from fcn.140017480 @ 0x14001756e(x)
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        
    loc_0x1400175ce:
        // CODE XREF from fcn.140017480 @ 0x1400175c7(x)
        r11 = var_60h
        rbx = qword [r11 + 0x20]
        rbp = qword [r11 + 0x28]
        rsp = r11
        r14 = pop () // rsp
        rdi = pop ()
        rsi = pop ()
        return
        goto loc_0x14001759a;
        goto loc_0x14001752a;
        return rax;
    loc_0x14001752a:
        eax++
        rbx = eax
        rbx += rbx   // reloc.OLEAUT32.dll_SysReAllocString
        rcx = rbx    // reloc.OLEAUT32.dll_SysReAllocString
        fcn.14004bee0 ()
        rdi = rax
        r8 = rbx     // reloc.OLEAUT32.dll_SysReAllocString
        rdx = qword [var_38h]
        rcx = rax
        fcn.140049380 () // fcn.140049380(0x1, 0x0, 0x1780b0)
    loc_0x140017570: // orphan
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         qword [var_48h] = 0
         byte [var_50h] = 0
         ecx = 2                  // reloc.OLEAUT32.dll_SysReAllocString
         fcn.140047b24 ()         // fcn.140047b24(0x0)
         qword [var_90h] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14001759a
    loc_0x14001759a: // orphan
         byte [rax] = bpl
         byte [rax + 1] = r14b

    loc_0x1400175a1: // orphan
         // CODE XREF from fcn.140017480 @ 0x140017598(x)
         qword [lpThreadId] = 0   // LPDWORD lpThreadId
         dword [dwCreationFlags] = 0 // DWORD dwCreationFlags
         r9 = rax                 // LPVOID lpParameter
         r8 = rip + fcn.1400178e0 // 0x1400178e0 // "H\x8b\xc4UATAUAVAWH\x8dh\xa1H\x81\xec\xe0" // LPTHREAD_START_ROUTINE lpStartAddress
         edx = 0                  // SIZE_T dwStackSize
         ecx = 0                  // LPSECURITY_ATTRIBUTES lpThreadAttributes
         qword [sym.imp.KERNEL32.dll_CreateThread] () // [0x14006a528:8]=0x9180c reloc.KERNEL32.dll_CreateThread // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CreateThread // HANDLE CreateThread(0, 0, 0x5541544155c48b48, 0, -1, -1)
         
    loc_0x1400175ce: // orphan
         // CODE XREF from fcn.140017480 @ 0x1400175c7(x)
         r11 = var_60h
         rbx = qword [r11 + 0x20]
         rbp = qword [r11 + 0x28]
         rsp = r11
         r14 = pop ()             // rsp
         rdi = pop ()
         rsi = pop ()
         return

        return rax;
}

```

## ime_dll_registration_check

```c
===== FUNCTION ime_dll_registration_check 0x140035df0 =====
[{"addr":5368929776,"name":"fcn.140035df0","size":198,"is-pure":"false","realsz":198,"noreturn":false,"recursive":false,"stackframe":128,"calltype":"ms","cost":68,"cc":4,"bits":64,"type":"fcn","nbbs":8,"tracecov":0,"is-lineal":true,"ninstrs":50,"edges":10,"ebbs":1,"signature":"fcn.140035df0 ();","minaddr":5368929776,"maxaddr":5368929974,"maxbbins":12,"midbbins":6.250,"ratbbins":1.920,"indegree":2,"outdegree":6,"nlocals":6,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.140035df0 (int rcx, int rdx) {
    loc_0x140035df0:
        // CALL XREF from fcn.140033250 @ 0x140033311(x)
        // CALL XREF from fcn.140034f90 @ 0x140035017(x)
        rsp -= 0x28
        edx = 0x100
        cl = 1
        0x140035e10 () // 0x140035e10(0x0, 0x0, 0x100, 0x1)
        edx = 0x200
        ecx = 0
        rsp += 0x28
        goto loc_0x140035e10
        
    loc_0x140035e10:
        // CODE XREFS from fcn.140035df0 @ 0x140035dfb(x), 0x140035e0b(x)
        push (rsi)
        rsp -= 0x50
        qword [var_20h] = 0xfffffffffffffffe
        qword [var_60h] = rbx
        ebx = edx
        esi = cl
        edx = cl
        rcx = var_28h // int64_t arg1
        fcn.140035ec0 () // fcn.140035ec0(0x0)
        v = dword [var_30h] - 0
        if (!v) goto loc_0x140035e9f // unlikely
        goto loc_0x140035e3e;
        return rax;
    loc_0x140035e3e:
        edx = ebx    // REGSAM samDesired
        rcx = var_38h // HKEY arg1
        fcn.140035ff0 () // fcn.140035ff0(0x0, 0x0)
        v = dword [var_40h] - 0
        if (!v) goto loc_0x140035e7d // unlikely
        goto loc_0x140035e52;
    loc_0x140035e7d:
        // CODE XREF from fcn.140035df0 @ 0x140035e50(x)
        r8d = 0
        edx = sil
        rcx = var_28h // int64_t arg1
        fcn.140036170 () // fcn.140036170(0x0)
        rbx = qword [var_28h]
        
    loc_0x140035e93:
        // CODE XREF from fcn.140035df0 @ 0x140035e7b(x)
        rcx = qword [var_38h] // (pstr 0xffffffff) "\n"
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        goto loc_0x140035ea4
        goto loc_0x140035e7d;
        return rax;
        return rax;
    loc_0x140035e52: // orphan
         rcx = rip + 0x3515f      // 0x14006afb8
         rdx = rcx
         rbx = qword [var_28h]
         v = rbx & rbx
         if (!zf) rdx = rbx
         v = qword [var_38h] - 0
         if (!zf) rcx = qword [var_38h] // (pstr 0xffffffff) "\n"
         fcn.1400539fc ()         // fcn.1400539fc(0x0, 0x0)
         v = eax & eax
         if (!v) 
    loc_0x140035e93: // orphan
         // CODE XREF from fcn.140035df0 @ 0x140035e7b(x)
         rcx = qword [var_38h]    // (pstr 0xffffffff) "\n"
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         
    loc_0x140035e9f: // orphan
         // CODE XREF from fcn.140035df0 @ 0x140035e3c(x)
         rbx = qword [var_28h]

    loc_0x140035ea4: // orphan
         // CODE XREF from fcn.140035df0 @ 0x140035e9d(x)
         rcx = rbx
         rbx = qword [var_60h]    // rsp
         rsp += 0x50
         rsi = pop ()
         
        return rax;
}

```

## ime_dll_path_controller

```c
===== FUNCTION ime_dll_path_controller 0x1400364e0 =====
[{"addr":5368931552,"name":"fcn.1400364e0","size":204,"is-pure":"false","realsz":204,"noreturn":false,"recursive":false,"stackframe":128,"calltype":"ms","cost":70,"cc":4,"bits":64,"type":"fcn","nbbs":7,"tracecov":0,"is-lineal":true,"ninstrs":53,"edges":9,"ebbs":1,"signature":"fcn.1400364e0 ();","minaddr":5368931552,"maxaddr":5368931756,"maxbbins":12,"midbbins":7.571,"ratbbins":1.585,"indegree":1,"outdegree":6,"nlocals":7,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.1400364e0 (int rcx, int rdx) {
    loc_0x1400364e0:
        // CALL XREF from fcn.140033250 @ 0x1400333fe(x)
        rsp -= 0x28
        edx = 0x100
        cl = 1
        0x140036500 () // 0x140036500(0x0, 0x0, 0x100, 0x1)
        edx = 0x200
        ecx = 0
        rsp += 0x28
        goto loc_0x140036500
        
    loc_0x140036500:
        // CODE XREFS from fcn.1400364e0 @ 0x1400364eb(x), 0x1400364fb(x)
        push (rdi)
        rsp -= 0x50
        qword [var_20h] = 0xfffffffffffffffe
        qword [var_60h] = rbx
        qword [var_68h] = rsi
        esi = cl
        rcx = var_28h
        fcn.140035ff0 () // fcn.140035ff0(0x0, 0x0)
        edi = dword [var_30h] // (pstr 0xffffffff) "\n"
        v = edi & edi
        if (v) goto loc_0x140036581 // likely
        goto loc_0x14003652f;
        return rax;
    loc_0x14003652f:
        edx = sil
        rcx = var_38h
        fcn.140035ec0 () // fcn.140035ec0(0x0)
        rbx = rax
        rax = var_28h
        v = rax - rbx
        if (!v) goto loc_0x140036573 // unlikely
        goto loc_0x14003654a;
    loc_0x140036573:
        // CODE XREF from fcn.1400364e0 @ 0x140036548(x)
        rcx = qword [var_38h] // (pstr 0xffffffff) "\n"
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        v = edi & edi
        if (!v) goto loc_0x140036593 // likely
        goto loc_0x140036581;
    loc_0x140036593:
        // CODE XREF from fcn.1400364e0 @ 0x14003657f(x)
        rcx = qword [var_28h]
        rbx = qword [var_60h] // rsp
        rsi = qword [var_68h]
        rsp += 0x50
        rdi = pop ()
        goto loc_0x14004bed8 // fcn.14004bed8 // fcn.14004bed8 // fcn.14004bed8(0x16)
        return rax;
    loc_0x14003654a: // orphan
         rcx = qword [var_28h]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = qword [rbx]
         qword [var_28h] = rcx
         edi = dword [rbx + 8]
         dword [var_30h] = edi
         ecx = dword [rbx + 0xc]
         dword [var_34h] = ecx
         eax = 0
         qword [rbx] = rax
         qword [rbx + 8] = rax

    loc_0x140036581: // orphan
         // CODE XREF from fcn.1400364e0 @ 0x14003652d(x)
         r8b = 1
         edx = sil
         rcx = var_28h
         fcn.140036170 ()         // fcn.140036170(0x0)

}

```

## ime_lifecycle

```c
===== FUNCTION ime_lifecycle 0x1400357a0 =====
[{"addr":5368928160,"name":"fcn.1400357a0","size":175,"is-pure":"false","realsz":175,"noreturn":false,"recursive":false,"stackframe":80,"calltype":"ms","cost":63,"cc":7,"bits":64,"type":"fcn","nbbs":13,"tracecov":0,"is-lineal":true,"ninstrs":37,"edges":18,"ebbs":1,"signature":"fcn.1400357a0 ();","minaddr":5368928160,"maxaddr":5368928335,"maxbbins":6,"midbbins":2.846,"ratbbins":2.108,"indegree":2,"outdegree":6,"nlocals":0,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.1400357a0 (int rcx, int rdx) {
    loc_0x1400357a0:
        // CALL XREF from fcn.140033250 @ 0x1400333f9(x)
        // CALL XREF from fcn.140034f90 @ 0x1400355c0(x)
        push (rbx)
        rsp -= 0x20
        rcx = qword [0x14009ca80] // [0x14009ca80:8]=0
        byte [0x14009ca98] = 0 // [0x14009ca98:1]=0
        v = rcx & rcx
        if (!v) goto loc_0x1400357bf // likely
        goto loc_0x1400357b9;
    loc_0x1400357bf:
        // CODE XREF from fcn.1400357a0 @ 0x1400357b7(x)
        rcx = qword [0x140096198] // [0x140096198:8]=0
        v = rcx & rcx
        if (!v) goto loc_0x1400357d6 // likely
        goto loc_0x1400357cb;
    loc_0x1400357d6:
        // CODE XREF from fcn.1400357a0 @ 0x1400357c9(x)
        rcx = qword [0x14009ca90] // [0x14009ca90:8]=0
        ebx = 0
        v = rcx & rcx
        if (!v) goto loc_0x1400357f1 // likely
        goto loc_0x1400357e4;
    loc_0x1400357f1:
        // CODE XREF from fcn.1400357a0 @ 0x1400357e2(x)
        rcx = qword [0x14009ca78] // [0x14009ca78:8]=0
        v = rcx & rcx
        if (!v) goto loc_0x14003580a // likely
        goto loc_0x1400357fd;
    loc_0x14003580a:
        // CODE XREF from fcn.1400357a0 @ 0x1400357fb(x)
        rcx = qword [0x14009ca80] // [0x14009ca80:8]=0
        v = rcx & rcx
        if (!v) goto loc_0x140035823 // likely
        goto loc_0x140035816;
    loc_0x140035823:
        // CODE XREF from fcn.1400357a0 @ 0x140035814(x)
        rcx = qword [0x14009ca88] // [0x14009ca88:8]=0
        v = rcx & rcx
        if (!v) goto loc_0x14003583c // likely
        goto loc_0x14003582f;
    loc_0x14003583c:
        // CODE XREF from fcn.1400357a0 @ 0x14003582d(x)
        rcx = rip + 0x6725d // 0x14009caa0
        rsp += 0x20
        rbx = pop ()
        goto loc_qword [sym.imp.KERNEL32.dll_DeleteCriticalSection] // [0x14006a480:8]=0x9199e reloc.KERNEL32.dll_DeleteCriticalSection // reloc.KERNEL32.dll_DeleteCriticalSection
        return rax;
    loc_0x1400357cb: // orphan
         edx = 0x7d0              // 2000 // DWORD dwMilliseconds // (pstr 0xffffffff) "\n"
         qword [sym.imp.KERNEL32.dll_WaitForSingleObject] () // [0x14006a518:8]=0x91828 reloc.KERNEL32.dll_WaitForSingleObject // "(\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_WaitForSingleObject // DWORD WaitForSingleObject(0, -1)

    loc_0x1400357e4: // orphan
         qword [sym.imp.KERNEL32.dll_UnmapViewOfFile] () // [0x14006a270:8]=0x91b56 reloc.KERNEL32.dll_UnmapViewOfFile // "V\x1b\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_UnmapViewOfFile // BOOL UnmapViewOfFile(0)
         qword [0x14009ca90] = rbx // [0x14009ca90:8]=0

    loc_0x1400357fd: // orphan
         qword [sym.imp.KERNEL32.dll_CloseHandle] () // [0x14006a510:8]=0x9183e reloc.KERNEL32.dll_CloseHandle // ">\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CloseHandle // BOOL CloseHandle(0)
         qword [0x14009ca78] = rbx // [0x14009ca78:8]=0

    loc_0x140035816: // orphan
         qword [sym.imp.KERNEL32.dll_CloseHandle] () // [0x14006a510:8]=0x9183e reloc.KERNEL32.dll_CloseHandle // ">\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CloseHandle // BOOL CloseHandle(0)
         qword [0x14009ca80] = rbx // [0x14009ca80:8]=0

    loc_0x14003582f: // orphan
         qword [sym.imp.KERNEL32.dll_CloseHandle] () // [0x14006a510:8]=0x9183e reloc.KERNEL32.dll_CloseHandle // ">\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_CloseHandle // BOOL CloseHandle(0)
         qword [0x14009ca88] = rbx // [0x14009ca88:8]=0

}

```

## startup_mode_controller

```c
===== FUNCTION startup_mode_controller 0x140036850 =====
[{"addr":5368932432,"name":"fcn.140036850","size":305,"is-pure":"false","realsz":305,"noreturn":false,"recursive":false,"stackframe":80,"calltype":"ms","cost":125,"cc":13,"bits":64,"type":"fcn","nbbs":16,"tracecov":0,"is-lineal":true,"ninstrs":82,"edges":23,"ebbs":3,"signature":"fcn.140036850 (int64_t arg1);","minaddr":5368932432,"maxaddr":5368932737,"maxbbins":13,"midbbins":5.125,"ratbbins":2.537,"indegree":9,"outdegree":13,"nlocals":1,"nargs":1,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.140036850 (int64_t arg1) {
    loc_0x140036850:
        // XREFS: CALL 0x140019abf  CALL 0x140023eb4  CALL 0x140032be1
        // XREFS: CALL 0x1400336fb  CALL 0x140033977  CALL 0x140033e55
        // XREFS: CALL 0x140034ee9  CALL 0x14003502f  CALL 0x140035170
        qword [var_8h] = rbx
        push (rdi)
        rsp -= 0x20
        rbx = rcx    // arg1
        fcn.140011450 () // fcn.140011450(0x0)
        rdx = qword gs:[0x58]
        edi = eax
        r8d = dword [0x1400a5bb8] // [0x1400a5bb8:4]=0x1c000 // (pstr 0xffffffff) "\n"
        eax = 4
        r8 = qword [rdx + r8*8]
        edx = dword [rax + r8]
        v = dword [0x14009cae0] - edx // [0x14009cae0:4]=0
        if (v > 0) goto loc_0x14003694d // unlikely
        goto loc_0x14003688d;
    loc_0x14003694d:
        // CODE XREF from fcn.140036850 @ 0x140036887(x)
        rcx = rip + 0x6618c // 0x14009cae0 // int64_t arg1
        fcn.1400479f4 () // fcn.1400479f4(0x0)
        v = dword [0x4009cae1] - 0xffffffff // [0x14009cae0:4]=0
        if (v) goto loc_0x14003688d // likely
        goto loc_0x140036966;
    loc_0x140036966:
        eax = byte [rbx + 0x39]
        rcx = rip + 0x6616f // 0x14009cae0 // int64_t arg1
        dword [0x14009cadc] = eax // [0x14009cadc:4]=0
        fcn.140047994 () // fcn.140047994(0x0)
        goto loc_0x14003688d
    loc_0x140036899: // orphan
         eax = byte [rbx + 0x39]
         v = dword [0x14009cadc] - eax // [0x14009cadc:4]=0
         if (!v) 
         goto loc_0x1400368a9
    loc_0x1400368a9: // orphan
         // CODE XREF from fcn.140036850 @ 0x140036897(x)
         eax = byte [rbx + 0x39]
         dword [0x14009cadc] = eax // [0x14009cadc:4]=0
         dword [0x1400961a8] = ecx // [0x1400961a8:4]=-1
         v = cl & cl
         if (!v) 
         goto loc_0x1400368bd
    loc_0x1400368bd: // orphan
         v = al & al
         if (!v) 
         goto loc_0x1400368c1
    loc_0x1400368c1: // orphan
         fcn.140016b70 ()
         v = al & al
         if (!v) 
    loc_0x1400368ca: // orphan
         edx = 0
         rcx = rbx
         fcn.140036600 ()
         dl = 1
         rcx = rbx
         fcn.140036990 ()
         v = eax & eax
         if (v) 
    loc_0x1400368e2: // orphan
         dl = 1
         rcx = rbx
         fcn.140036600 ()
         eax = edi
         rbx = qword [var_8h]
         rsp += 0x20
         rdi = pop ()
         return

        return rax;
    loc_0x1400368f9: // orphan
         // CODE XREFS from fcn.140036850 @ 0x1400368bf(x), 0x1400368c8(x)
         fcn.140016b70 ()
         v = al & al
         if (!v) 
         goto loc_0x140036902
    loc_0x140036902: // orphan
         edx = 0
         rcx = rbx
         fcn.140036990 ()

    loc_0x14003690c: // orphan
         // CODE XREF from fcn.140036850 @ 0x140036900(x)
         dl = 1
         rcx = rbx
         fcn.140036600 ()
         eax = edi
         rbx = qword [var_8h]
         rsp += 0x20
         rdi = pop ()
         return

        return rax;
    loc_0x140036923: // orphan
         // CODE XREF from fcn.140036850 @ 0x1400368bb(x)
         edx = 0
         rcx = rbx
         fcn.140036600 ()
         fcn.140016b70 ()
         v = al & al
         if (!v) 
    loc_0x140036936: // orphan
         edx = 0
         rcx = rbx
         fcn.140036990 ()

    loc_0x140036940: // orphan
         // CODE XREFS from fcn.140036850 @ 0x1400368a3(x), 0x1400368e0(x), 0x140036934(x)
         rbx = qword [var_8h]
         eax = edi
         rsp += 0x20
         rdi = pop ()
         return

        return rax;
}

```

## scheduled_task_create_delete

```c
===== FUNCTION scheduled_task_create_delete 0x140036990 =====
[{"addr":5368932752,"name":"fcn.140036990","size":3105,"is-pure":"false","realsz":3105,"noreturn":false,"recursive":false,"stackframe":2304,"calltype":"ms","cost":1037,"cc":53,"bits":64,"type":"fcn","nbbs":96,"tracecov":0,"is-lineal":true,"ninstrs":811,"edges":147,"ebbs":1,"signature":"fcn.140036990 ();","minaddr":5368932752,"maxaddr":5368935857,"maxbbins":53,"midbbins":8.448,"ratbbins":6.274,"indegree":3,"outdegree":77,"nlocals":20,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.140036990 (int rcx, int rdx) {
    loc_0x140036990:
        // CALL XREFS from fcn.140036850 @ 0x1400368d9(x), 0x140036907(x), 0x14003693b(x)
        rax = rsp
        push (rdi)
        push (r12)
        push (r13)
        push (r14)
        push (r15)
        rsp -= 0x8b0
        qword [rax - 0x840] = 0xfffffffffffffffe
        qword [rax + 8] = rbx
        qword [rax + 0x10] = rbp
        qword [rax + 0x18] = rsi
        rax = qword [0x14009a9e8] // [0x14009a9e8:8]=0x2b992ddfa200
        rax ^= rsp
        qword [var_8a0h] = rax // [0x8a0:8]=-1 // 2208
        ebx = dl
        rcx = var_78h // int64_t arg1
        fcn.140016270 () // fcn.140016270(0x0)
        edi = 0
        r14d = edi
        qword [var_88h] = rdi
        qword [var_90h] = rdi
        r15d = edi
        dword [var_38h] = edi
        v = bl & bl
        if (!v) goto loc_0x14003731d // likely
        goto loc_0x1400369fe;
    loc_0x14003731d:
        // CODE XREF from fcn.140036990 @ 0x1400369f8(x)
        edx = 0x20
        ecx = 0
        fcn.14004bee8 ()
        r13d = 0x10
        v = rax & rax
        if (!v) goto loc_0x14003733a // likely
        goto loc_0x140037334;
    loc_0x14003733a:
        // CODE XREF from fcn.140036990 @ 0x140037332(x)
        xmm0 = xmmword [str._Delete__TN] // [0x140076698:16]=-1 // u"/Delete /TN "
        xmmword [r14] = xmm0
        xmm1 = qword [0x1400766a8] // [0x1400766a8:8]=0x20004e0054002f
        qword [r14 + 0x10] = xmm1
        eax = word [0x1400766b0] // [0x1400766b0:2]=0
        word [r14 + 0x18] = ax
        ecx = 0x38   // '8' // u(cstr 0x00000000) "/Delete "
        fcn.14004bee0 ()
        rsi = rax
        qword [var_50h] = rax
        xmm0 = xmmword [0x1400766b8] // [0x1400766b8:16]=-1
        xmmword [rax] = xmm0
        xmm1 = xmmword [0x1400766c8] // [0x1400766c8:16]=-1
        xmmword [rax + 0x10] = xmm1
        xmm0 = xmmword [0x1400766d8] // [0x1400766d8:16]=-1
        xmmword [rax + 0x20] = xmm0
        xmm1 = qword [0x1400766e8] // [0x1400766e8:8]=0x6400720061
        qword [rax + 0x30] = xmm1
        ecx = 4      // u"Key - Vietnamese Keybo"
        fcn.14004bee0 ()
        rdi = rax
        ecx = dword [0x14007658c] // [0x14007658c:4]=34 // u"e Keybo"
        dword [rax] = ecx
        r12 = rip + 0x33bfe // 0x14006afb8 // (pstr 0x00000000) "\""
        rbx = r12    // (pstr 0x00000000) "\""
        v = rsi & rsi
        if (!zf) rbx = rsi
        ebp = 1
        v = rbx & rbx
        if (!v) goto loc_0x140037410 // unlikely
        goto loc_0x1400373ce;
    loc_0x140037410:
        // CODE XREF from fcn.140036990 @ 0x1400373cc(x)
        eax = 0
        
    loc_0x140037412:
        // CODE XREF from fcn.140036990 @ 0x14003740e(x)
        rsi = rax
        ebx = eax
        v = rdi & rdi
        if (!v) goto loc_0x140037447 // likely
        goto loc_0x14003741c;
        goto loc_0x140036a25;
        return rax;
    loc_0x140036a25:
        // CODE XREFS from fcn.140036990 @ 0x14003758e(x), 0x1400375ac(x)
        r12 = rip + 0x3458c // 0x14006afb8 // u(pstr 0x00000000) "\"\""
        ebp = 1
        r13d = rbp + 0xf // u"  ietnameee Keyboo"
        r15 = 0xffffffffffffffff
        eax = dword [0x14009cad0] // [0x14009cad0:4]=60 // u(cstr 0x00000000) "\"\""
        v = eax & eax
        if (v) goto loc_0x140036de1 // likely
        goto loc_0x140036a4a;
    loc_0x140036de1:
        // CODE XREF from fcn.140036990 @ 0x140036a44(x)
        rsi = rdi
        qword [var_50h] = rdi
        dword [var_58h] = edi
        ebx = edi
        dword [var_30h] = ebx
        dword [var_5ch] = ebx
        v = eax & eax
        if (!v) goto loc_0x14003730e // likely
        goto loc_0x140036dff;
    loc_0x14003730e:
        // CODE XREF from fcn.140036990 @ 0x140036df9(x)
        ebp = edi
        
    loc_0x140037310:
        // CODE XREFS from fcn.140036990 @ 0x140036e20(x), 0x14003730a(x)
        rcx = rsi
        fcn.14004bed8 () // fcn.14004bed8(0x0)
        goto loc_0x140037535
        goto loc_0x140036e19;
        goto loc_0x140036b25;
        return rax;
    loc_0x140036b25:
        dword [var_40h] = ebx
        v = edi & edi // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
        if (v <= 0) goto loc_0x140036b82 // likely
        goto loc_0x140036b2d;
    loc_0x140036b82:
        // CODE XREFS from fcn.140036990 @ 0x140036b23(x), 0x140036b2b(x)
        edx = 0      // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
        
    loc_0x140036b84:
        // CODE XREF from fcn.140036990 @ 0x140036b80(x)
        rdi = rdx
        qword [var_48h] = rdx
        eax = edx
        dword [var_34h] = edx
        ecx = edx
        v = ebx & ebx // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
        if (v <= 0) goto loc_0x140036bd8 // likely
        goto loc_0x140036b98;
        goto loc_0x140036b3c;
        return rax;
    loc_0x140036b3c:
        v = eax - 4  // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
        ecx = 4
        cmovle eax ecx
        v = eax - r13d
        if (v < 0) eax = r13d
        rdx = eax
        rdx += rdx   // reloc.OLEAUT32.dll_VariantClear
        rcx = rsi
        fcn.14004bee8 ()
        v = rax & rax
        if (!zf) rsi = rax
        
    loc_0x140036b63:
        // CODE XREF from fcn.140036990 @ 0x140036b3a(x)
        r8 = rdi
        r8 += r8     // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
        rcx = rsi + 2 // (pstr 0x00000000) "\"" reloc.OLEAUT32.dll_SysReAllocString
        rdx = qword [var_48h]
        fcn.140049380 () // fcn.140049380(0x0, 0x0, 0x0)
        rax = ebx
        edx = 0      // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
        word [rsi + rax*2] = dx
        goto loc_0x140036b84
        return rax;
    loc_0x140036a4a: // orphan
         ecx = 0                  // LPCWSTR lpModuleName
         qword [sym.imp.KERNEL32.dll_GetModuleHandleW] () // [0x14006a1d8:8]=0x919fa reloc.KERNEL32.dll_GetModuleHandleW // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GetModuleHandleW // HMODULE GetModuleHandleW(0)
         rdi = rax
         r8 = rip + 0x3fb34       // u"BIN" // 0x140076590 // LPCWSTR lpType
         edx = r13 + 0x7c
         rcx = rax                // HMODULE hModule
         qword [sym.imp.KERNEL32.dll_FindResourceW] () // [0x14006a558:8]=0x917b4 reloc.KERNEL32.dll_FindResourceW // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_FindResourceW // HRSRC FindResourceW(0, "", "B")
         rbx = rax
         rdx = rax                // HRSRC hResInfo
         rcx = rdi                // HMODULE hModule
         qword [sym.imp.KERNEL32.dll_SizeofResource] () // [0x14006a560:8]=0x917a2 reloc.KERNEL32.dll_SizeofResource // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_SizeofResource // DWORD SizeofResource(0, 0)
         rsi = eax
         rdx = rbx                // HRSRC hResInfo
         rcx = rdi                // HMODULE hModule
         qword [sym.imp.KERNEL32.dll_LoadResource] () // [0x14006a568:8]=0x91792 reloc.KERNEL32.dll_LoadResource // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_LoadResource // HGLOBAL LoadResource(0, 0)
         rdi = rax
         ecx = rsi*2 + 2          // reloc.OLEAUT32.dll_SysReAllocString
         rbx = ecx                // reloc.OLEAUT32.dll_SysReAllocString
         eax = 0
         qword [var_60h] = rax
         qword [var_68h] = rax
         qword [var_70h] = rax
         rdx = rbx                // uint32_t arg2 // reloc.OLEAUT32.dll_SysReAllocString
         rcx = var_60h            // uint32_t arg1
         fcn.140004800 ()         // fcn.140004800(0x0, 0x0)
         r8 = rbx                 // reloc.OLEAUT32.dll_SysReAllocString
         edx = 0
         rcx = qword [var_60h]
         fcn.140049090 ()         // fcn.140049090(0x0, 0x0)
         rcx = rdi                // HGLOBAL hResData
         qword [sym.imp.KERNEL32.dll_LockResource] () // [0x14006a570:8]=0x91782 reloc.KERNEL32.dll_LockResource // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_LockResource // LPVOID LockResource(0)
         rdx = rax
         r8 = rsi
         rcx = qword [var_60h]
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rdx = qword [var_60h]    // LPWSTR arg2
         rcx = rip + 0x65fe2      // 0x14009cac8 // int64_t arg1
         fcn.140014e80 ()         // fcn.140014e80(0x0, 0x0)
         ecx = rbp + 3
         fcn.14004bee0 ()
         rsi = rax
         ecx = dword [0x14007655c] // [0x14007655c:4]=34 // u"ee Keyboo"
         dword [rax] = ecx
         rax = r12
         rcx = qword [var_78h]
         v = rcx & rcx            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (!zf) rax = rcx
         qword [var_48h] = rax
         rdi = dword [var_80h]
         ebx = ebp                // u(pstr 0x00000016) "etnameee Keyboo" rsp
         dword [var_40h] = ebx
         v = rax & rax            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (!v) 
    loc_0x140036b2d: // orphan
         ebx = rdi + 1            // (pstr 0x00000000) "\""
         dword [var_40h] = ebx
         eax = rbx + 1            // (pstr 0x00000000) "\"" reloc.OLEAUT32.dll_SysReAllocString
         v = eax - 2              // reloc.OLEAUT32.dll_SysReAllocString // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (v <= 0) 
    loc_0x140036b63: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036b3a(x)
         r8 = rdi
         r8 += r8                 // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         rcx = rsi + 2            // (pstr 0x00000000) "\"" reloc.OLEAUT32.dll_SysReAllocString
         rdx = qword [var_48h]
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rax = ebx
         edx = 0                  // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         word [rsi + rax*2] = dx
         
    loc_0x140036b84: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036b80(x)
         rdi = rdx
         qword [var_48h] = rdx
         eax = edx
         dword [var_34h] = edx
         ecx = edx
         v = ebx & ebx
         if (v <= 0) 
         goto loc_0x140036b98
    loc_0x140036b98: // orphan
         dword [var_34h] = edx
         v = rsi & rsi
         if (!v) 
    loc_0x140036ba1: // orphan
         eax = rbx + 1
         dword [var_30h] = eax
         rbx = eax
         rbx += rbx               // reloc.OLEAUT32.dll_SysReAllocString
         rcx = rbx                // reloc.OLEAUT32.dll_SysReAllocString
         fcn.14004bee0 ()
         rdi = rax
         qword [var_48h] = rax
         r8 = rbx                 // reloc.OLEAUT32.dll_SysReAllocString
         rdx = rsi
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x1, 0x0, 0x0)
         eax = dword [var_40h]
         dword [var_34h] = eax
         ecx = dword [var_30h]

    loc_0x140036bd8: // orphan
         // CODE XREFS from fcn.140036990 @ 0x140036b96(x), 0x140036b9f(x)
         eax += 2                 // reloc.OLEAUT32.dll_SysReAllocString
         v = eax - ecx
         if (v <= 0) 
         goto loc_0x140036bdf
    loc_0x140036bdf: // orphan
         ecx += ecx
         v = ecx - eax
         cmovge eax ecx
         v = eax - r13d
         if (v < 0) eax = r13d
         rdx = eax
         rdx += rdx
         rcx = rdi
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) rdi = rax
         qword [var_48h] = rdi

    loc_0x140036c07: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036bdd(x)
         rax = dword [var_34h]
         rcx = rdi + rax*2
         eax = dword [0x14007656c] // [0x14007656c:4]=34 // u"ee Keyboo"
         dword [rcx] = eax
         rcx = rsi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         ecx = 0x18               // u"tnameee Keyboo"
         fcn.14004bee0 ()
         rbx = rax                // u"ee Keyboo"
         qword [var_50h] = rax
         xmm0 = xmmword [0x140076598] // [0x140076598:16]=-1
         xmmword [rax] = xmm0
         xmm1 = qword [0x1400765a8] // [0x1400765a8:8]=0x5d00520049
         qword [rax + 0x10] = xmm1
         rdx = r12
         v = rax & rax            // (pstr 0x00000000) "\""
         if (!zf) rdx = rax       // u"[EVKey_D"
         rcx = qword [0x14009cac8] // [0x14009cac8:8]=0x8033003c80320000
         v = rcx & rcx            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (!v) 
         goto loc_0x140036c63
    loc_0x140036c63: // orphan
         v = rdx & rdx            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (!v) 
    loc_0x140036c6c: // orphan
         v = dword [0x14009cad0] - 0 // [0x14009cad0:4]=60 // (pstr 0x00000000) "\""
         if (v <= 0) 
    loc_0x140036c79: // orphan
         fcn.1400497a8 ()         // fcn.1400497a8(0x0, 0x0, 0x0, 0x0, 0x0)
         rsi = rax
         v = rax & rax            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (!v) 
    loc_0x140036c8a: // orphan
         r9 = qword [0x14009cac8] // [0x14009cac8:8]=0x8033003c80320000
         rsi -= r9                // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         rsi >>= 1                // (pstr 0x00000000) "\""
         v = esi - r15d           // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (!v) 
    loc_0x140036ca0: // orphan
         rdx = r12
         v = rdi & rdi            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (!zf) rdx = rdi
         qword [var_40h] = rdx
         r10d = 0xb
         dword [var_34h] = r10d
         v = esi & esi            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         js 0x140036dbf           // unlikely

    loc_0x140036cc2: // orphan
         ecx = dword [0x14009cad0] // [0x14009cad0:4]=60 // (cstr 0x00000000) "\""
         v = esi - ecx            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (v > 0) 
    loc_0x140036cd0: // orphan
         v = rdx & rdx            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (!v) 
    loc_0x140036cd9: // orphan
         eax = rsi + 0xb
         v = eax - ecx
         if (v <= 0) 
         goto loc_0x140036ce0
    loc_0x140036ce0: // orphan
         r10d = ecx
         r10d -= esi              // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         dword [var_34h] = r10d

    loc_0x140036ceb: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036cde(x)
         rbx = r15

    loc_0x140036cf0: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036cf8(x)
         rbx++                    // (pstr 0x00000000) "\""
         v = word [rdx + rbx*2] - 0 // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (v) 
         goto loc_0x140036cfa
    loc_0x140036cfa: // orphan
         eax = ebx
         eax -= r10d              // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         eax += ecx               // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         dword [var_30h] = eax
         edi = rax + 1            // (pstr 0x00000000) "\""
         eax = dword [0x14009cad4] // [0x14009cad4:4]=0x4e0000 // (pstr 0xffffffff) "\n"
         v = edi - eax            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (v <= 0) 
         goto loc_0x140036d12
    loc_0x140036d12: // orphan
         eax += eax               // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         v = eax - edi            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         cmovge edi eax
         v = edi - r13d           // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (v < 0) edi = r13d
         rdx = edi
         rdx += rdx               // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         rcx = r9
         fcn.14004bee8 ()
         r9 = rax
         ecx = dword [0x14009cad0] // [0x14009cad0:4]=60 // (cstr 0x00000000) "\""
         r10d = dword [var_34h]
         v = rax & rax            // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         if (!v) 
         goto loc_0x140036d41
    loc_0x140036d41: // orphan
         qword [0x14009cac8] = rax // [0x14009cac8:8]=0x8033003c80320000
         dword [0x14009cad4] = edi // [0x14009cad4:4]=0x4e0000
         
    loc_0x140036d50: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036d3f(x)
         r9 = qword [0x14009cac8] // [0x14009cac8:8]=0

    loc_0x140036d57: // orphan
         // CODE XREFS from fcn.140036990 @ 0x140036d10(x), 0x140036d4e(x)
         rdi = esi
         rbx = ebx
         ecx -= r10d              // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         ecx -= esi               // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         ecx++                    // (pstr 0x00000000) "\""
         r8 = ecx                 // (pstr 0x00000000) "\""
         r8 += r8                 // (pstr 0x00000000) "\"" reloc.OLEAUT32.dll_SysReAllocString
         rax = r10d
         rax += rdi               // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         rdx = r9 + rax*2
         rax = rdi + rbx
         rcx = r9 + rax*2
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         r8 = rbx + rbx
         rax = qword [0x14009cac8] // [0x14009cac8:8]=0
         rcx = rax + rdi*2
         rdx = qword [var_40h]    // (pstr 0xffffffff) "\n"
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x1400812)
         rax = dword [var_30h]
         dword [0x14009cad0] = eax // [0x14009cad0:4]=60
         rcx = rax
         rax = qword [0x14009cac8] // [0x14009cac8:8]=0
         edx = 0                  // (pstr 0x00000000) "\"" // (pstr 0x00000000) "\""
         word [rax + rcx*2] = dx
         rdi = qword [var_48h]    // (pstr 0xffffffff) "\n"
         rbx = qword [var_50h]

    loc_0x140036dbf: // orphan
         // XREFS: CODE 0x140036c5d  CODE 0x140036c66  CODE 0x140036c73   // XREFS: CODE 0x140036c84  CODE 0x140036c9a  CODE 0x140036cbc   // XREFS: CODE 0x140036cca  CODE 0x140036cd3  
         rcx = rbx
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = qword [var_60h]
         fcn.1400474e4 ()
         eax = dword [0x14009cad0] // [0x14009cad0:4]=0
         edi = 0

    loc_0x140036dff: // orphan
         rcx = r12                // int64_t arg1
         rax = qword [0x14009cac8] // [0x14009cac8:8]=0
         v = rax & rax
         if (!zf) rcx = rax
         fcn.140018460 ()         // fcn.140018460(0x0)
         v = al & al
         if (!v) 
         goto loc_0x140036e19
    loc_0x140036e19: // orphan
         fcn.140018170 ()
         v = al & al
         if (v) 
         goto loc_0x140036e26
    loc_0x140036e26: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036e17(x)
         rdx = lpBuffer           // LPWSTR lpBuffer
         ecx = 0x400              // DWORD nBufferLength
         qword [sym.imp.KERNEL32.dll_GetTempPathW] () // [0x14006a1e8:8]=0x91a24 reloc.KERNEL32.dll_GetTempPathW // "$\x1a\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GetTempPathW // DWORD GetTempPathW(0x00000000, 0x0000000000000000)
         rax = lpBuffer

    loc_0x140036e41: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036e4a(x)
         r15++
         v = word [rax + r15*2] - 0
         if (v) 
         goto loc_0x140036e4c
    loc_0x140036e4c: // orphan
         edi = r15 + 1
         v = edi & edi
         if (v <= 0) 
         goto loc_0x140036e54
    loc_0x140036e54: // orphan
         ebx = edi
         edx = 0
         cmovle ebx edx
         v = ebx - 0x10
         if (v < 0) ebx = r13d
         rdx = ebx
         rdx += rdx
         ecx = 0
         fcn.14004bee8 ()
         v = rax & rax
         if (!v) 
         goto loc_0x140036e77
    loc_0x140036e77: // orphan
         rsi = rax
         
    loc_0x140036e7c: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036e75(x)
         ebx = dword [var_30h]

    loc_0x140036e80: // orphan
         // CODE XREFS from fcn.140036990 @ 0x140036e52(x), 0x140036e7a(x)
         r8 = edi
         r8 += r8
         rdx = lpBuffer
         rcx = rsi
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x1780a0)
         edi = r15 + 0xb
         ecx = rdi + 1            // u" -  ietname[EVKey_D"
         v = ecx - ebx
         if (v <= 0) 
         goto loc_0x140036ea1
    loc_0x140036ea1: // orphan
         eax = rbx + rbx
         v = eax - ecx
         cmovge ecx eax
         v = ecx - 0x10
         if (v < 0) ecx = r13d
         rdx = ecx
         rdx += rdx
         rcx = rsi
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) rsi = rax

    loc_0x140036ec5: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036e9f(x)
         rax = r15d
         xmm0 = xmmword [str.evktask.xml] // [0x1400765b0:16]=-1 // u"evktask.xml"
         xmmword [rsi + rax*2] = xmm0
         xmm1 = qword [0x1400765c0] // [0x1400765c0:8]=0x6c006d0078
         qword [rsi + rax*2 + 0x10] = xmm1
         rdx = r12
         v = rsi & rsi
         if (!zf) rdx = rsi       // int64_t arg2
         r8 = rip + str.wt_ccsUTF_16LE // 0x1400765c8 // u"wt+,ccs=UTF-16LE" // int64_t arg3
         rcx = var_48h            // int64_t arg1 // u(cstr 0x00000000) "evktask."
         fcn.1400542b8 ()         // fcn.1400542b8(0x0, 0x0, 0x0)
         r9 = qword [var_48h]
         v = r9 & r9
         if (!v) 
         goto loc_0x140036f0a
    loc_0x140036f0a: // orphan
         rcx = r12                // int64_t arg1
         rax = qword [0x14009cac8] // [0x14009cac8:8]=0
         v = rax & rax
         if (!zf) rcx = rax
         r8 = dword [0x14009cad0] // [0x14009cad0:4]=0 // uint32_t arg3
         edx = 2                  // int64_t arg2 // u"vktask." reloc.OLEAUT32.dll_SysReAllocString
         fcn.1400549d8 ()         // fcn.1400549d8(0x0, 0x0, 0x2, 0x0)
         rcx = qword [var_48h]    // uint32_t arg1
         fcn.140054394 ()         // fcn.140054394(0x0)
         edx = 0x20               // u"e[EVKey_D"
         ecx = 0
         fcn.14004bee8 ()
         v = rax & rax
         if (!v) 
         goto loc_0x140036f47
    loc_0x140036f47: // orphan
         r14 = rax
         dword [var_38h] = r13d

    loc_0x140036f4f: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036f45(x)
         dword [var_34h] = 0xd    // [0xd:4]=0x2e00
         xmm0 = xmmword [str._Create__XML] // [0x1400765f0:16]=-1 // u"/Create /XML "
         xmmword [r14] = xmm0
         xmm1 = qword [0x140076600] // [0x140076600:8]=0x4c004d0058002f
         qword [r14 + 0x10] = xmm1
         eax = dword [0x140076608] // [0x140076608:4]=32 // u"e[EVKey_D"
         dword [r14 + 0x18] = eax
         ecx = 4                  // u"reate "
         fcn.14004bee0 ()
         r15 = rax                // u"e[EVKey_D"
         ecx = dword [0x14007657c] // [0x14007657c:4]=34 // u"[EVKey_D"
         dword [rax] = ecx
         rbx = r12
         v = rsi & rsi
         if (!zf) rbx = rsi
         v = rbx & rbx
         if (!v) 
         goto loc_0x140036f9e
    loc_0x140036f9e: // orphan
         v = edi & edi
         if (v <= 0) 
    loc_0x140036fa2: // orphan
         ebp = rdi + 1
         eax = rbp + 1            // u"Create " reloc.OLEAUT32.dll_SysReAllocString
         v = eax - 2              // reloc.OLEAUT32.dll_SysReAllocString // u"Create "
         if (v <= 0) 
         goto loc_0x140036fad
    loc_0x140036fad: // orphan
         v = eax - 4
         ecx = 4                  // u"reate "
         cmovle eax ecx           // u"reate "
         v = eax - 0x10
         if (v < 0) eax = r13d
         rdx = eax
         rdx += rdx
         rcx = r15
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) r15 = rax

    loc_0x140036fd4: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036fab(x)
         r8 = edi
         r8 += r8
         rcx = r15 + 2            // u"Create " reloc.OLEAUT32.dll_SysReAllocString
         rdx = rbx
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rax = ebp                // rsp
         ecx = 0
         word [r15 + rax*2] = cx
         
         goto loc_0x140036ff2
    loc_0x140036ff2: // orphan
         // CODE XREFS from fcn.140036990 @ 0x140036f9c(x), 0x140036fa0(x)
         ecx = 0

    loc_0x140036ff4: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036ff0(x)
         rdi = rcx
         ebx = ecx
         eax = ecx
         v = ebp & ebp
         if (v <= 0) 
         goto loc_0x140036fff
    loc_0x140036fff: // orphan
         v = r15 & r15
         if (!v) 
    loc_0x140037004: // orphan
         eax = rbp + 1            // u(pstr 0x00000000) "/Create "
         dword [var_30h] = eax
         rbx = eax                // u(pstr 0x00000000) "/Create "
         rbx += rbx               // (pstr 0xffffffff) "\n"
         rcx = rbx                // (pstr 0xffffffff) "\n"
         fcn.14004bee0 ()
         rdi = rax                // u(pstr 0x00000000) "/Create "
         r8 = rbx                 // (pstr 0xffffffff) "\n"
         rdx = r15
         rcx = rax                // u(pstr 0x00000000) "/Create "
         fcn.140049380 ()         // fcn.140049380(0x178001, 0x0, 0x0)
         ebx = ebp                // rsp
         eax = dword [var_30h]

    loc_0x140037030: // orphan
         // CODE XREFS from fcn.140036990 @ 0x140036ffd(x), 0x140037002(x)
         ebp = rbx + 2            // u"Create " reloc.OLEAUT32.dll_SysReAllocString
         dword [var_30h] = ebp
         ecx = rbp + 1
         v = ecx - eax
         if (v <= 0) 
         goto loc_0x14003703e
    loc_0x14003703e: // orphan
         eax += eax
         v = eax - ecx
         cmovge ecx eax
         v = ecx - 0x10
         if (v < 0) ecx = r13d
         rdx = ecx
         rdx += rdx
         rcx = rdi
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) rdi = rax

    loc_0x140037061: // orphan
         // CODE XREF from fcn.140036990 @ 0x14003703c(x)
         rax = ebx
         rcx = rdi + rax*2
         eax = dword [0x14007660c] // [0x14007660c:4]=0x200022 // (pstr 0xffffffff) "\n"
         dword [rcx] = eax
         eax = word [0x140076610] // [0x140076610:2]=0
         word [rcx + 4] = ax
         rax = r12
         v = rdi & rdi
         if (!zf) rax = rdi
         qword [var_50h] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14003708f
    loc_0x14003708f: // orphan
         v = ebp & ebp
         if (v <= 0) 
    loc_0x140037093: // orphan
         eax = rbp + 0xd          // u(pstr 0x00000000) "\" "
         dword [var_34h] = eax
         ebx = rax + 1
         ebp = dword [var_38h]
         v = ebx - ebp
         if (v <= 0) 
         goto loc_0x1400370a5
    loc_0x1400370a5: // orphan
         eax = rbp*2
         v = eax - ebx
         cmovge ebx eax
         v = ebx - 0x10
         if (v < 0) ebx = r13d
         rdx = ebx
         rdx += rdx               // (pstr 0xffffffff) "\n"
         rcx = r14
         fcn.14004bee8 ()
         v = rax & rax
         if (!v) 
    loc_0x1400370cb: // orphan
         r14 = rax
         dword [var_38h] = ebx

    loc_0x1400370d2: // orphan
         // CODE XREFS from fcn.140036990 @ 0x1400370a3(x), 0x1400370c9(x)
         r8 = dword [var_30h]     // (pstr 0x00000020) "\"" reloc.OLEAUT32.dll_SysReAllocString
         r8 += r8
         rcx = r14 + 0x1a
         rdx = qword [var_50h]
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rax = dword [var_34h]
         edx = 0
         word [r14 + rax*2] = dx

    loc_0x1400370f4: // orphan
         // CODE XREFS from fcn.140036990 @ 0x14003708d(x), 0x140037091(x)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = r15
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         ecx = 0x38               // '8'
         fcn.14004bee0 ()
         rbp = rax
         xmm0 = xmmword [0x140076618] // [0x140076618:16]=-1
         xmmword [rax] = xmm0
         xmm1 = xmmword [0x140076628] // [0x140076628:16]=-1
         xmmword [rax + 0x10] = xmm1
         xmm0 = xmmword [0x140076638] // [0x140076638:16]=-1
         xmmword [rax + 0x20] = xmm0
         xmm1 = qword [0x140076648] // [0x140076648:8]=0x6400720061
         qword [rax + 0x30] = xmm1
         ecx = 0xc                // u"- Vietnamese Keybo"
         fcn.14004bee0 ()
         rdi = rax
         xmm0 = qword [str._TN__] // [0x140076650:8]=0x20004e0054002f // u"/TN \""
         qword [rax] = xmm0
         ecx = dword [0x140076658] // [0x140076658:4]=34 // u"e Keybo"
         dword [rax + 8] = ecx
         rbx = r12
         v = rbp & rbp
         if (!zf) rbx = rbp       // u(pstr 0x00000016) "tnamese Keybo" rsp
         ecx = 5
         dword [var_40h] = ecx
         v = rbx & rbx
         if (!v) 
         goto loc_0x140037178
    loc_0x140037178: // orphan
         edx = rcx + 0x3d
         rcx = rax
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) rdi = rax
         xmm0 = xmmword [rbx]
         xmmword [rdi + 0xa] = xmm0
         xmm1 = xmmword [rbx + 0x10]
         xmmword [rdi + 0x1a] = xmm1
         xmm0 = xmmword [rbx + 0x20]
         xmmword [rdi + 0x2a] = xmm0
         eax = dword [rbx + 0x30] // (pstr 0xffffffff) "\n"
         dword [rdi + 0x3a] = eax
         eax = word [rbx + 0x34]
         word [rdi + 0x3e] = ax
         eax = 0
         word [rdi + 0x40] = ax
         ecx = 0x20               // u"- ame- ameybomey"
         dword [var_40h] = ecx
         
         goto loc_0x1400371c0
    loc_0x1400371c0: // orphan
         // CODE XREF from fcn.140036990 @ 0x140037176(x)
         eax = 0

    loc_0x1400371c2: // orphan
         // CODE XREF from fcn.140036990 @ 0x1400371be(x)
         r15 = rax
         ebx = eax
         v = rdi & rdi
         if (!v) 
         goto loc_0x1400371cc
    loc_0x1400371cc: // orphan
         eax = rcx + 1
         dword [var_30h] = eax
         ebx = eax
         rbx += rbx               // reloc.OLEAUT32.dll_SysReAllocString
         rcx = rbx                // reloc.OLEAUT32.dll_SysReAllocString
         fcn.14004bee0 ()
         r15 = rax
         r8 = rbx                 // reloc.OLEAUT32.dll_SysReAllocString
         rdx = rdi
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         ebx = dword [var_40h]
         eax = dword [var_30h]

    loc_0x1400371f9: // orphan
         // CODE XREF from fcn.140036990 @ 0x1400371ca(x)
         edx = rbx + 4
         dword [var_30h] = edx
         ecx = rdx + 1
         v = ecx - eax
         if (((unsigned) v) <= 0) 
         goto loc_0x140037207
    loc_0x140037207: // orphan
         eax += eax
         v = eax - ecx
         cmovae ecx eax
         v = ecx - 0x10
         cmovb ecx r13d
         edx = ecx
         rdx += rdx
         rcx = r15
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) r15 = rax
         edx = rbx + 4

    loc_0x14003722c: // orphan
         // CODE XREF from fcn.140036990 @ 0x140037205(x)
         eax = ebx
         rcx = r15 + rax*2
         xmm0 = qword [0x140076660] // [0x140076660:8]=0x46002f00200022
         qword [rcx] = xmm0
         eax = word [0x140076668] // [0x140076668:2]=0
         word [rcx + 8] = ax
         rax = r12
         v = r15 & r15
         if (!zf) rax = r15
         qword [var_50h] = rax
         v = rax & rax
         if (!v) 
         goto loc_0x14003725d
    loc_0x14003725d: // orphan
         rbx = dword [var_34h]
         eax = rbx + rdx
         dword [var_34h] = eax
         ecx = rax + 1
         eax = dword [var_38h]    // u"- ame- ameybomey"
         v = ecx - eax
         if (v <= 0) 
         goto loc_0x140037274
    loc_0x140037274: // orphan
         eax += eax
         v = eax - ecx
         cmovge ecx eax
         v = ecx - 0x10
         if (v < 0) ecx = r13d
         rdx = ecx
         rdx += rdx
         rcx = r14
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) r14 = rax
         edx = dword [var_30h]

    loc_0x14003729b: // orphan
         // CODE XREF from fcn.140036990 @ 0x140037272(x)
         r8d = edx
         r8 += r8
         rcx = r14 + rbx*2
         rdx = qword [var_50h]
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rax = dword [var_34h]
         ebx = 0
         word [r14 + rax*2] = bx
         
         goto loc_0x1400372bd
    loc_0x1400372bd: // orphan
         // CODE XREF from fcn.140036990 @ 0x14003725b(x)
         ebx = 0

    loc_0x1400372bf: // orphan
         // CODE XREF from fcn.140036990 @ 0x1400372bb(x)
         rcx = r15
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = rbp                // u(pstr 0x00000016) "- " rsp
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         v = r14 & r14
         if (!zf) r12 = r14
         dword [nShowCmd] = ebx   // INT nShowCmd
         qword [lpDirectory] = rbx // LPCWSTR lpDirectory
         r9 = r12                 // LPCWSTR lpParameters
         r8 = rip + 0x3f37f       // u"SCHTASKS" // 0x140076670 // LPCWSTR lpFile
         rdx = rip + 0x3f390      // u"runas" // 0x140076688 // LPCWSTR lpOperation
         ecx = 0                  // HWND hwnd
         qword [sym.imp.SHELL32.dll_ShellExecuteW] () // [0x14006a5f8:8]=0x927d0 reloc.SHELL32.dll_ShellExecuteW // (pstr 0xffffffff) "\n" reloc.SHELL32.dll_ShellExecuteW // HINSTANCE ShellExecuteW(0, "r", "S", 0, -1, -1)
         ebp = ebx
         v = rax - 0x20           // u"- ame- ameybomey"
         setg bpl
         
    loc_0x14003730c: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036f04(x)
         edi = 0

    loc_0x140037310: // orphan
         // CODE XREFS from fcn.140036990 @ 0x140036e20(x), 0x14003730a(x)
         rcx = rsi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         
    loc_0x140037334: // orphan
         r14 = rax
         r15d = r13d

    loc_0x1400373ce: // orphan
         edx = rbp + 0x39
         rcx = rax
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) rdi = rax
         xmm0 = xmmword [rbx]
         xmmword [rdi + 2] = xmm0
         xmm1 = xmmword [rbx + 0x10]
         xmmword [rdi + 0x12] = xmm1
         xmm0 = xmmword [rbx + 0x20]
         xmmword [rdi + 0x22] = xmm0
         eax = dword [rbx + 0x30] // (pstr 0xffffffff) "\n"
         dword [rdi + 0x32] = eax
         eax = word [rbx + 0x34]
         word [rdi + 0x36] = ax
         eax = 0
         word [rdi + 0x38] = ax
         ebp = rax + 0x1c
         
    loc_0x140037412: // orphan
         // CODE XREF from fcn.140036990 @ 0x14003740e(x)
         rsi = rax
         ebx = eax
         v = rdi & rdi
         if (!v) 
         goto loc_0x14003741c
    loc_0x14003741c: // orphan
         eax = rbp + 1
         dword [var_30h] = eax
         ebx = eax
         rbx += rbx               // (pstr 0xffffffff) "\n"
         rcx = rbx                // (pstr 0xffffffff) "\n"
         fcn.14004bee0 ()
         rsi = rax
         r8 = rbx                 // (pstr 0xffffffff) "\n"
         rdx = rdi
         rcx = rax
         fcn.140049380 ()         // fcn.140049380(0x0, 0x178001, 0x0)
         ebx = ebp                // rsp
         eax = dword [var_30h]

    loc_0x140037447: // orphan
         // CODE XREF from fcn.140036990 @ 0x14003741a(x)
         ebp = rbx + 4
         ecx = rbp + 1
         v = ecx - eax
         if (((unsigned) v) <= 0) 
         goto loc_0x140037451
    loc_0x140037451: // orphan
         eax += eax
         v = eax - ecx
         cmovae ecx eax
         v = ecx - r13d
         cmovb ecx r13d
         edx = ecx
         rdx += rdx
         rcx = rsi
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) rsi = rax

    loc_0x140037473: // orphan
         // CODE XREF from fcn.140036990 @ 0x14003744f(x)
         eax = ebx
         rcx = rsi + rax*2
         xmm0 = qword [str.___F]  // [0x1400766f0:8]=0x46002f00200022 // u"\" /F"
         qword [rcx] = xmm0
         eax = word [0x1400766f8] // [0x1400766f8:2]=0
         word [rcx + 8] = ax
         rbx = r12
         v = rsi & rsi
         if (!zf) rbx = rsi
         v = rbx & rbx
         if (!v) 
         goto loc_0x14003749f
    loc_0x14003749f: // orphan
         ecx = rbp + 0xd
         v = ecx - r15d
         if (v <= 0) 
         goto loc_0x1400374a7
    loc_0x1400374a7: // orphan
         eax = r15 + r15
         v = eax - ecx
         cmovge ecx eax
         v = ecx - r13d
         if (v < 0) ecx = r13d
         rdx = ecx
         rdx += rdx
         rcx = r14
         fcn.14004bee8 ()
         v = rax & rax
         if (!zf) r14 = rax

    loc_0x1400374cc: // orphan
         // CODE XREF from fcn.140036990 @ 0x1400374a5(x)
         r8d = ebp                // rsp
         r8 += r8
         rcx = r14 + 0x18         // u"- "
         rdx = rbx
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         ecx = rbp + 0xc
         ebx = 0
         word [r14 + rcx*2] = bx

    loc_0x1400374e8: // orphan
         // CODE XREF from fcn.140036990 @ 0x14003749d(x)
         rcx = rsi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = rdi
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = qword [var_50h]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         v = r14 & r14
         if (!zf) r12 = r14
         dword [nShowCmd] = ebx   // INT nShowCmd
         qword [lpDirectory] = rbx // LPCWSTR lpDirectory
         r9 = r12                 // LPCWSTR lpParameters
         r8 = rip + str.SCHTASKS  // 0x140076700 // u"SCHTASKS" // LPCWSTR lpFile
         rdx = rip + str.runas    // 0x140076718 // u"runas" // LPCWSTR lpOperation
         ecx = 0                  // HWND hwnd
         qword [sym.imp.SHELL32.dll_ShellExecuteW] () // [0x14006a5f8:8]=0x927d0 reloc.SHELL32.dll_ShellExecuteW // (pstr 0xffffffff) "\n" reloc.SHELL32.dll_ShellExecuteW // HINSTANCE ShellExecuteW(0, "r", "S", 0, -1, -1)
         ebp = ebx
         v = rax - 0x20
         setg bpl

    loc_0x140037535: // orphan
         // CODE XREF from fcn.140036990 @ 0x140037318(x)
         rcx = r14
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         rcx = qword [var_78h]
         fcn.14004bed8 ()         // fcn.14004bed8(0x0)
         eax = ebp                // rsp
         rcx = qword [var_8a0h]
         rcx ^= rsp
         fcn.140047380 ()         // fcn.140047380(0x0)
         r11 = var_8b0h
         rbx = qword [r11 + 0x30]
         rbp = qword [r11 + 0x38]
         rsi = qword [r11 + 0x40]
         rsp = r11
         r15 = pop ()
         r14 = pop ()
         r13 = pop ()
         r12 = pop ()
         rdi = pop ()
         return

        return rax;
    loc_0x14003757b: // orphan
         // CODE XREF from fcn.140036990 @ 0x140036a1f(x)
         rcx = rip + 0x65556      // 0x14009cad8 // int64_t arg1
         fcn.1400479f4 ()         // fcn.1400479f4(0x0)
         v = dword [0x4009cad9] - 0xffffffff // [0x14009cad8:4]=0
         if (v) 
         goto loc_0x140037594
    loc_0x140037594: // orphan
         rcx = rip + 0x31d65      // 0x140069300
         fcn.1400477c4 ()         // fcn.1400477c4(0x0)
         rcx = rip + 0x65531      // 0x14009cad8 // int64_t arg1
         fcn.140047994 ()         // fcn.140047994(0x0)
         
}

```

## raw_input_thread

```c
===== FUNCTION raw_input_thread 0x140034700 =====
[{"addr":5368923904,"name":"fcn.140034700","size":94,"is-pure":"false","realsz":94,"noreturn":false,"recursive":false,"stackframe":80,"calltype":"ms","cost":37,"cc":4,"bits":64,"type":"fcn","nbbs":6,"tracecov":0,"is-lineal":true,"ninstrs":23,"edges":8,"ebbs":1,"signature":"fcn.140034700 ();","minaddr":5368923904,"maxaddr":5368923998,"maxbbins":8,"midbbins":3.833,"ratbbins":2.087,"indegree":0,"outdegree":4,"nlocals":0,"nargs":0,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
int fcn.140034700 (int rcx, int rdx) {
    loc_0x140034700:
        // DATA XREF from fcn.140034f90 @ 0x140035379(r)
        rsp -= 0x28
        al = 1
        byte [0x14009c898] = al // [0x14009c898:1]=0
        
    loc_0x140034710:
        // CODE XREF from fcn.140034700 @ 0x140034752(x)
        rcx = qword [0x14009c918] // [0x14009c918:8]=0
        v = rcx & rcx
        if (!v) goto loc_0x14003472e // likely
        goto loc_0x14003471c;
        return rax;
    loc_0x14003471c:
        edx = 0xffffffff // -1 // DWORD dwMilliseconds // "\n"
        qword [sym.imp.KERNEL32.dll_WaitForSingleObject] () // [0x14006a518:8]=0x91828 reloc.KERNEL32.dll_WaitForSingleObject // "(\x18\t" // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_WaitForSingleObject // DWORD WaitForSingleObject(0, 0x0a0a000a)
        eax = byte [0x14009c898] // [0x14009c898:1]=1
        
    loc_0x14003472e:
        // CODE XREF from fcn.140034700 @ 0x14003471a(x)
        v = al & al
        if (!v) goto loc_0x140034754 // likely
        goto loc_0x140034732;
        return rax;
    loc_0x140034732:
        ecx = 0x64   // 'd' // DWORD dwMilliseconds
        sub.KERNEL32.dll_Sleep () // VOID Sleep(0x00000000)
        sub.USER32.dll_GetForegroundWindow () // HWND GetForegroundWindow(void)
        rcx = rax
        fcn.140008d90 () // fcn.140008d90(0x0)
        eax = byte [0x14009c898] // [0x14009c898:1]=1
        v = al & al
        if (v) goto loc_0x140034710 // likely
        goto loc_0x140034754;
    loc_0x140034754:
        // CODE XREF from fcn.140034700 @ 0x140034730(x)
        eax = 1
        rsp += 0x28
        return
        return rax;
    loc_0x14003472e: // orphan
         // CODE XREF from fcn.140034700 @ 0x14003471a(x)
         v = al & al
         if (!v) 
}

```

## clipboard_send_helpers

```c
===== FUNCTION clipboard_send_helpers 0x1400045e0 =====
[{"addr":5368727008,"name":"fcn.1400045e0","size":431,"is-pure":"false","realsz":431,"noreturn":false,"recursive":false,"stackframe":144,"calltype":"ms","cost":167,"cc":13,"bits":64,"type":"fcn","nbbs":19,"tracecov":0,"is-lineal":true,"ninstrs":110,"edges":30,"ebbs":1,"signature":"fcn.1400045e0 (HWND *hWndNewOwner);","minaddr":5368727008,"maxaddr":5368727439,"maxbbins":16,"midbbins":5.789,"ratbbins":2.764,"indegree":1,"outdegree":16,"nlocals":6,"nargs":1,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.1400045e0 (HWND *hWndNewOwner) {
    loc_0x1400045e0:
        // CALL XREF from fcn.140004a60 @ 0x140004a80(x)
        push (rbx)
        push (rsi)
        push (rdi)
        push (r13)
        rsp -= 0x48
        rax = qword [0x14009a9e8] // [0x14009a9e8:8]=0x2b992ddfa200
        rax ^= rsp
        qword [var_30h] = rax
        rsi = qword [rcx + 0x38] // arg1
        r13 = rcx    // arg1
        ebx = 0
        rcx = rsi    // HWND hWndNewOwner
        edi = ebx
        qword [sym.imp.USER32.dll_OpenClipboard] () // [0x14006a868:8]=0x91d02 reloc.USER32.dll_OpenClipboard // u(pstr 0x00000018) "- " // (pstr 0xffffffff) "\n" reloc.USER32.dll_OpenClipboard // BOOL OpenClipboard(-1)
        v = eax & eax
        if (v) goto loc_0x14000463c // likely
        goto loc_0x140004611;
    loc_0x14000463c:
        // CODE XREFS from fcn.1400045e0 @ 0x14000460f(x), 0x140004632(x)
        qword [var_78h] = rbp
        rcx = r13
        qword [var_80h] = r12
        qword [var_88h] = r14
        qword [var_40h] = r15
        fcn.140004790 () // fcn.140004790(0x0)
        xmm0 = xmmword [0x14006af90] // [0x14006af90:16]=-1
        rcx = r13 + 0x40
        edx = 4
        xmmword [var_20h] = xmm0
        fcn.1400048d0 () // fcn.1400048d0(0x0, 0x0)
        qword [r13 + 0x48] = 4
        r15 = var_20h
        r14 = rbx
        
    loc_0x140004690:
        // CODE XREF from fcn.1400045e0 @ 0x140004726(x)
        rsi = qword [r13 + 0x40]
        edi = dword [r15]
        rsi += r14
        dword [rsi] = edi
        v = edi - 0xe
        if (((unsigned) v) <= 0) goto 0x1400046b7 // likely
        goto loc_0x1400046a1;
        goto loc_0x140004619;
        return rax;
    loc_0x140004619:
        ecx = 1      // DWORD dwMilliseconds
        edi++
        sub.KERNEL32.dll_Sleep () // VOID Sleep(0x00000000)
        rcx = rsi    // HWND hWndNewOwner
        qword [sym.imp.USER32.dll_OpenClipboard] () // [0x14006a868:8]=0x91d02 reloc.USER32.dll_OpenClipboard // "%F" // (pstr 0xffffffff) "\n" reloc.USER32.dll_OpenClipboard // BOOL OpenClipboard(0)
        v = eax & eax
        if (!v) goto loc_0x140004611 // likely
        goto loc_0x140004632;
    loc_0x140004632:
        goto loc_0x14000463c
    loc_0x140004634: // orphan
         // CODE XREF from fcn.1400045e0 @ 0x140004617(x)
         v = eax & eax
         if (!v) 
    loc_0x140004690: // orphan
         // CODE XREF from fcn.1400045e0 @ 0x140004726(x)
         rsi = qword [r13 + 0x40]
         edi = dword [r15]
         rsi += r14
         dword [rsi] = edi
         v = edi - 0xe
         if (((unsigned) v) <= 0) 
         goto loc_0x1400046a1
    loc_0x1400046a1: // orphan
         r8d = 0x64               // 'd' // int cchMaxCount
         rdx = rsi + 4            // LPWSTR lpszFormatName
         ecx = edi                // UINT format
         qword [sym.imp.USER32.dll_GetClipboardFormatNameW] () // [0x14006a890:8]=0x91d68 reloc.USER32.dll_GetClipboardFormatNameW // "h\x1d\t" // ".F" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetClipboardFormatNameW // int GetClipboardFormatNameW(0, 0x0000000000000000, 0x00000000)
         v = eax & eax
         if (v) 
         goto loc_0x1400046b7
    loc_0x1400046b7: // orphan
         // CODE XREF from fcn.1400045e0 @ 0x14000469f(x)
         word [rsi + 4] = bx

    loc_0x1400046bb: // orphan
         // CODE XREF from fcn.1400045e0 @ 0x1400046b5(x)
         ecx = edi                // UINT uFormat
         qword [sym.imp.USER32.dll_GetClipboardData] () // [0x14006a880:8]=0x91d38 reloc.USER32.dll_GetClipboardData // "8\x1d\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_GetClipboardData // HANDLE GetClipboardData(0)
         r12 = rax
         v = rax & rax
         if (!v) 
         goto loc_0x1400046cb
    loc_0x1400046cb: // orphan
         rcx = rax                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalSize] () // [0x14006a548:8]=0x917d2 reloc.KERNEL32.dll_GlobalSize // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalSize // SIZE_T GlobalSize(0)
         rcx = r12                // HGLOBAL hMem
         rbp = rax
         qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
         rdx = rbp                // rsp
         rcx = rsi + 0xd0
         rdi = rax
         fcn.140004800 ()         // fcn.140004800(0x0, 0x0)
         qword [rsi + 0xd8] = rbp
         r8 = rbp                 // rsp
         rcx = qword [rsi + 0xd0]
         rdx = rdi
         fcn.140049380 ()         // fcn.140049380(0x0, 0x0, 0x0)
         rcx = r12                // HGLOBAL hMem
         qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)

    loc_0x140004714: // orphan
         // CODE XREF from fcn.1400045e0 @ 0x1400046c9(x)
         r15 += 4
         r14 += 0xe8
         v = r14 - 0x3a0
         jl 0x140004690           // likely

         goto loc_0x14000472c
    loc_0x14000472c: // orphan
         qword [sym.imp.USER32.dll_CloseClipboard] () // [0x14006a870:8]=0x91d12 reloc.USER32.dll_CloseClipboard // (pstr 0xffffffff) "\n" reloc.USER32.dll_CloseClipboard // BOOL CloseClipboard(void)
         r15 = qword [var_40h]
         r14 = qword [var_88h]
         r12 = qword [var_80h]    // rsp
         rbp = qword [var_78h]
         v = eax & eax
         if (v) 
         goto loc_0x140004750
    loc_0x140004750: // orphan
         // CODE XREF from fcn.1400045e0 @ 0x14000476c(x)
         v = ebx - 0x3e8          // 1000
         if (v >= 0) 
         goto loc_0x140004758
    loc_0x140004758: // orphan
         ecx = 1                  // DWORD dwMilliseconds
         ebx++
         sub.KERNEL32.dll_Sleep () // VOID Sleep(0x00000000)
         qword [sym.imp.USER32.dll_CloseClipboard] () // [0x14006a870:8]=0x91d12 reloc.USER32.dll_CloseClipboard // "dG" // (pstr 0xffffffff) "\n" reloc.USER32.dll_CloseClipboard // BOOL CloseClipboard(void)
         v = eax & eax
         if (!v) 
         goto loc_0x14000476e
    loc_0x14000476e: // orphan
         // CODE XREFS from fcn.1400045e0 @ 0x14000474e(x), 0x140004789(x)
         al = 1

    loc_0x140004770: // orphan
         // CODE XREF from fcn.1400045e0 @ 0x14000478d(x)
         rcx = qword [var_30h]
         rcx ^= rsp
         fcn.140047380 ()         // fcn.140047380(0x0)
         rsp += 0x48
         r13 = pop ()
         rdi = pop ()
         rsi = pop ()
         rbx = pop ()
         return

        return rax;
    loc_0x140004787: // orphan
         // CODE XREF from fcn.1400045e0 @ 0x140004756(x)
         v = eax & eax
         if (v) 
    loc_0x14000478b: // orphan
         // CODE XREF from fcn.1400045e0 @ 0x140004636(x)
         al = 0
         
}

```

## clipboard_set_helper

```c
===== FUNCTION clipboard_set_helper 0x140004b20 =====
[{"addr":5368728352,"name":"fcn.140004b20","size":290,"is-pure":"false","realsz":290,"noreturn":false,"recursive":false,"stackframe":96,"calltype":"ms","cost":138,"cc":13,"bits":64,"type":"fcn","nbbs":18,"tracecov":0,"is-lineal":true,"ninstrs":84,"edges":29,"ebbs":1,"signature":"fcn.140004b20 (HWND *hWndNewOwner, int64_t arg2, int64_t arg3);","minaddr":5368728352,"maxaddr":5368728642,"maxbbins":17,"midbbins":4.667,"ratbbins":3.643,"indegree":3,"outdegree":14,"nlocals":3,"nargs":3,"difftype":"new"}]
// callconv: rax ms (rcx, rdx, r8, r9, stack);
void fcn.140004b20 (HWND *hWndNewOwner, int64_t arg2, int64_t arg3) {
    loc_0x140004b20:
        // CALL XREF from fcn.140007b50 @ 0x140007c11(x)
        // CALL XREFS from fcn.140008010 @ 0x140008095(x), 0x1400082da(x)
        qword [var_8h] = rbx
        qword [var_10h] = rbp
        qword [var_18h] = rsi
        push (rdi)
        push (r14)
        push (r15)
        rsp -= 0x20
        rsi = qword [rcx + 0x38] // arg1
        edi = 0
        rcx = rsi    // HWND hWndNewOwner
        rbp = r9d    // arg4
        r14d = r8b
        r15 = rdx    // arg2
        ebx = edi
        qword [sym.imp.USER32.dll_OpenClipboard] () // [0x14006a868:8]=0x91d02 reloc.USER32.dll_OpenClipboard // (pstr 0xffffffff) "\n" reloc.USER32.dll_OpenClipboard // BOOL OpenClipboard(-1)
        v = eax & eax
        if (v) goto loc_0x140004b82 // unlikely
        goto loc_0x140004b57;
    loc_0x140004b82:
        // CODE XREFS from fcn.140004b20 @ 0x140004b55(x), 0x140004b78(x)
        ebx = edi
        qword [sym.imp.USER32.dll_EmptyClipboard] () // [0x14006a898:8]=0x91d82 reloc.USER32.dll_EmptyClipboard // (pstr 0xffffffff) "\n" reloc.USER32.dll_EmptyClipboard // BOOL EmptyClipboard(void)
        v = eax & eax
        if (v) goto loc_0x140004bae // unlikely
        goto loc_0x140004b8e;
    loc_0x140004bae:
        // CODE XREFS from fcn.140004b20 @ 0x140004b8c(x), 0x140004b96(x)
        rdx = rbp    // SIZE_T dwBytes // rsp
        ecx = 2      // UINT uFlags // reloc.OLEAUT32.dll_SysReAllocString
        qword [sym.imp.KERNEL32.dll_GlobalAlloc] () // [0x14006a550:8]=0x917c4 reloc.KERNEL32.dll_GlobalAlloc // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalAlloc // HGLOBAL GlobalAlloc(0x00000000, 0x178000)
        rcx = rax    // HGLOBAL hMem
        rsi = rax
        qword [sym.imp.KERNEL32.dll_GlobalLock] () // [0x14006a540:8]=0x917e0 reloc.KERNEL32.dll_GlobalLock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalLock // LPVOID GlobalLock(0)
        r8 = rbp     // rsp
        rdx = r15
        rcx = rax
        fcn.140049380 () // fcn.140049380(0x0, 0x0, 0x0)
        rcx = rsi    // HGLOBAL hMem
        qword [sym.imp.KERNEL32.dll_GlobalUnlock] () // [0x14006a538:8]=0x917ee reloc.KERNEL32.dll_GlobalUnlock // (pstr 0xffffffff) "\n" reloc.KERNEL32.dll_GlobalUnlock // BOOL GlobalUnlock(0)
        rdx = rsi
        ecx = 0xd
        v = r14b & r14b
        if (v) goto loc_0x140004bf1 // unlikely
        goto loc_0x140004bec;
    loc_0x140004bf1:
        // CODE XREF from fcn.140004b20 @ 0x140004bea(x)
        qword [sym.imp.USER32.dll_SetClipboardData] () // [0x14006a878:8]=0x91d24 reloc.USER32.dll_SetClipboardData // "$\x1d\t" // (pstr 0xffffffff) "\n" reloc.USER32.dll_SetClipboardData // HANDLE SetClipboardData(0, 0)
        qword [sym.imp.USER32.dll_CloseClipboard] () // [0x14006a870:8]=0x91d12 reloc.USER32.dll_CloseClipboard // (pstr 0xffffffff) "\n" reloc.USER32.dll_CloseClipboard // BOOL CloseClipboard(void)
        v = eax & eax
        if (v) goto loc_0x140004c1f // unlikely
        goto loc_0x140004c01;
    loc_0x140004c1f:
        // CODE XREFS from fcn.140004b20 @ 0x140004bff(x), 0x140004c3c(x)
        al = 1
        
    loc_0x140004c21:
        // CODE XREF from fcn.140004b20 @ 0x140004c40(x)
        rbx = qword [var_8h]
        rbp = qword [var_10h]
        rsi = qword [var_18h]
        rsp += 0x20
        r15 = pop ()
        r14 = pop ()
        rdi = pop ()
        return
        goto loc_0x140004c09;
        goto loc_0x140004b5f;
        return rax;
    loc_0x140004b5f:
        ecx = 1      // DWORD dwMilliseconds
        ebx++
        sub.KERNEL32.dll_Sleep () // VOID Sleep(0x00000000)
        rcx = rsi    // HWND hWndNewOwner
        qword [sym.imp.USER32.dll_OpenClipboard] () // [0x14006a868:8]=0x91d02 reloc.USER32.dll_OpenClipboard // "kK" // (pstr 0xffffffff) "\n" reloc.USER32.dll_OpenClipboard // BOOL OpenClipboard(0)
        v = eax & eax
        if (!v) goto loc_0x140004b57 // likely
        goto loc_0x140004b78;
    loc_0x140004b78:
        goto loc_0x140004b82
    loc_0x140004b7a: // orphan
         // CODE XREF from fcn.140004b20 @ 0x140004b5d(x)
         v = eax & eax
         if (!v) 
    loc_0x140004b8e: // orphan
         

    loc_0x140004b90: // orphan
         // CODE XREF from fcn.140004b20 @ 0x140004bac(x)
         v = ebx - 0x3e8          // 1000
         if (v >= 0) 
         goto loc_0x140004b98
    loc_0x140004b98: // orphan
         ecx = 1                  // DWORD dwMilliseconds
         ebx++
         sub.KERNEL32.dll_Sleep () // VOID Sleep(0x00000000)
         qword [sym.imp.USER32.dll_EmptyClipboard] () // [0x14006a898:8]=0x91d82 reloc.USER32.dll_EmptyClipboard // (pstr 0xffffffff) "\n" reloc.USER32.dll_EmptyClipboard // BOOL EmptyClipboard(void)
         v = eax & eax
         if (!v) 
    loc_0x140004bec: // orphan
         ecx = 1

    loc_0x140004c01: // orphan
         // CODE XREF from fcn.140004b20 @ 0x140004c1d(x)
         v = edi - 0x3e8          // 1000
         if (v >= 0) 
         goto loc_0x140004c09
    loc_0x140004c09: // orphan
         ecx = 1                  // DWORD dwMilliseconds
         edi++
         sub.KERNEL32.dll_Sleep () // VOID Sleep(0x00000000)
         qword [sym.imp.USER32.dll_CloseClipboard] () // [0x14006a870:8]=0x91d12 reloc.USER32.dll_CloseClipboard // (pstr 0xffffffff) "\n" reloc.USER32.dll_CloseClipboard // BOOL CloseClipboard(void)
         v = eax & eax
         if (!v) 
    loc_0x140004c21: // orphan
         // CODE XREF from fcn.140004b20 @ 0x140004c40(x)
         rbx = qword [var_8h]
         rbp = qword [var_10h]
         rsi = qword [var_18h]
         rsp += 0x20
         r15 = pop ()
         r14 = pop ()
         rdi = pop ()
         return

        return rax;
    loc_0x140004c3a: // orphan
         // CODE XREF from fcn.140004b20 @ 0x140004c07(x)
         v = eax & eax
         if (v) 
         goto loc_0x140004c3e
    loc_0x140004c3e: // orphan
         // CODE XREF from fcn.140004b20 @ 0x140004b7c(x)
         al = 0
         
}

```
