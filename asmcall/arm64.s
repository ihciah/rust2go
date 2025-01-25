//go:build arm64
// +build arm64

// Copyright 2024 ihciah. All Rights Reserved.

#include "textflag.h"

#define G0ASMCALL                                         \
    /* save SP */                                         \
    MOVD    RSP, R4                                       \
    /* read g.0 and g.m.g0 */                             \
    MOVD    0x30(g), R3            /* g.m */              \
    MOVD    0x0(R3), R3            /* g.m.g0 */           \
    /* mark unpreemptible by replacing g with g0 */       \
    MOVD    g, R5                                         \
    MOVD    R3, g                                         \
    /* switch SP to g0 and align stack */                 \
    MOVD    0x38(R3), R3           /* g.m.g0.sched */     \
    AND     $~15, R3                                      \
    MOVD    R3, RSP                                       \
    /* push SP and original g */                          \
    SUB     $0x20, RSP                                    \
    MOVD    R30, 0x10(RSP)                                \
    STP     (R5, R4), (RSP)                               \
    /* call the function */                               \
    CALL    R8                                            \
    /* restore g and SP */                                \
    LDP     (RSP), (g, R3)                                \
    MOVD    0x10(RSP), R30                                \
    MOVD    R3, RSP                                       \
    RET

#define ASMCALL                                           \
    CALL    R8                                            \
    RET

TEXT ·CallFuncG0P0(SB), NOSPLIT|NOPTR|NOFRAME, $0
    MOVD    fn+0x0(FP), R8
    G0ASMCALL

TEXT ·CallFuncG0P1(SB), NOSPLIT|NOPTR|NOFRAME, $0
    MOVD    fn+0x0(FP), R8
    MOVD    arg0+0x8(FP), R0
    G0ASMCALL

TEXT ·CallFuncG0P2(SB), NOSPLIT|NOPTR|NOFRAME, $0
    MOVD    fn+0x0(FP), R8
    MOVD    arg0+0x8(FP), R0
    MOVD    arg1+0x10(FP), R1
    G0ASMCALL

TEXT ·CallFuncG0P3(SB), NOSPLIT|NOPTR|NOFRAME, $0
    MOVD    fn+0x0(FP), R8
    MOVD    arg0+0x8(FP), R0
    MOVD    arg1+0x10(FP), R1
    MOVD    arg1+0x18(FP), R2
    G0ASMCALL

TEXT ·CallFuncP0(SB), NOSPLIT|NOPTR|NOFRAME, $0
    MOVD    fn+0x0(FP), R8
    ASMCALL

TEXT ·CallFuncP1(SB), NOSPLIT|NOPTR|NOFRAME, $0
    MOVD    fn+0x0(FP), R8
    MOVD    arg0+0x8(FP), R0
    ASMCALL

TEXT ·CallFuncP2(SB), NOSPLIT|NOPTR|NOFRAME, $0
    MOVD    fn+0x0(FP), R8
    MOVD    arg0+0x8(FP), R0
    MOVD    arg1+0x10(FP), R1
    ASMCALL

TEXT ·CallFuncP3(SB), NOSPLIT|NOPTR|NOFRAME, $0
    MOVD    fn+0x0(FP), R8
    MOVD    arg0+0x8(FP), R0
    MOVD    arg1+0x10(FP), R1
    MOVD    arg1+0x18(FP), R2
    ASMCALL
