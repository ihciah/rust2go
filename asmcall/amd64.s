//go:build amd64
// +build amd64

// Copyright 2017 petermattis. Copyright 2024 ihciah. All Rights Reserved.
// Part of code is borrowed from github.com/petermattis/fastcgo/call_amd64.s

#include "textflag.h"

#ifdef GOOS_windows
#define RARG0 CX
#define RARG1 DX
#define RARG2 R8
#define RTMP0 R9
#define RTMP1 R10
#define RTMP2 R11
#else
#define RARG0 DI
#define RARG1 SI
#define RARG2 DX
#define RTMP0 R8
#define RTMP1 R9
#define RTMP2 R10
#endif

#define G0ASMCALL                                         \
    /* save SP */                                         \
    MOVQ    SP, RTMP0                                     \
                                                          \
    /* read g.0 and g.m.g0 */                             \
    MOVQ    0x30(g), RTMP1             /* g.m */          \
    MOVQ    0x0(RTMP1), RTMP1          /* g.m.g0 */       \
                                                          \
    /* mark unpreemptible by replacing g with g0 */       \
    MOVQ    g, RTMP2                                      \
    MOVQ    RTMP1, g                                      \
                                                          \
    /* switch SP to g0 and align stack */                 \
    MOVQ    0x38(RTMP1), SP            /* g.m.g0.sched */ \
    ANDQ    $-16, SP                                      \
                                                          \
    /* push SP and original g */                          \
    PUSHQ   RTMP0                                         \
    PUSHQ   RTMP2                                         \
                                                          \
    /* call the function */                               \
    CALL    AX                                            \
                                                          \
    /* restore g and SP */                                \
    POPQ    g                                             \
    POPQ    SP                                            \
    RET

#define ASMCALL                                           \
    CALL    AX                                            \
    RET

TEXT ·CallFuncG0P0(SB), NOSPLIT|NOPTR|NOFRAME, $0
    // save SP and read parameters
    MOVQ    fn+0x0(FP), AX
    G0ASMCALL

TEXT ·CallFuncG0P1(SB), NOSPLIT|NOPTR|NOFRAME, $0
    // save SP and read parameters
    MOVQ    fn+0x0(FP), AX
    MOVQ    arg0+0x8(FP), RARG0
    G0ASMCALL

TEXT ·CallFuncG0P2(SB), NOSPLIT|NOPTR|NOFRAME, $0
    // save SP and read parameters
    MOVQ    fn+0x0(FP), AX
    MOVQ    arg0+0x8(FP), RARG0
    MOVQ    arg1+0x10(FP), RARG1
    G0ASMCALL

TEXT ·CallFuncG0P3(SB), NOSPLIT|NOPTR|NOFRAME, $0
    // save SP and read parameters
    MOVQ    fn+0x0(FP), AX
    MOVQ    arg0+0x8(FP), RARG0
    MOVQ    arg1+0x10(FP), RARG1
    MOVQ    arg1+0x18(FP), RARG2
    G0ASMCALL

TEXT ·CallFuncP0(SB), NOSPLIT|NOPTR|NOFRAME, $0
    // save SP and read parameters
    MOVQ    fn+0x0(FP), AX
    ASMCALL

TEXT ·CallFuncP1(SB), NOSPLIT|NOPTR|NOFRAME, $0
    // save SP and read parameters
    MOVQ    fn+0x0(FP), AX
    MOVQ    arg0+0x8(FP), RARG0
    ASMCALL

TEXT ·CallFuncP2(SB), NOSPLIT|NOPTR|NOFRAME, $0
    // save SP and read parameters
    MOVQ    fn+0x0(FP), AX
    MOVQ    arg0+0x8(FP), RARG0
    MOVQ    arg1+0x10(FP), RARG1
    ASMCALL

TEXT ·CallFuncP3(SB), NOSPLIT|NOPTR|NOFRAME, $0
    // save SP and read parameters
    MOVQ    fn+0x0(FP), AX
    MOVQ    arg0+0x8(FP), RARG0
    MOVQ    arg1+0x10(FP), RARG1
    MOVQ    arg1+0x18(FP), RARG2
    ASMCALL
