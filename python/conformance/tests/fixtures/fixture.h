/* A header in the shape a package's generated header has, for the self-test. */
#ifndef FIX_H
#define FIX_H

#include <stdint.h>

#define FIX_ABI_VERSION 3

#define FIX_OK 0
#define FIX_ERR_NULL -1
#define FIX_ERR_RANGE -2
#define FIX_ERR_UTF8 -3
#define FIX_ERR_PANIC -4
#define FIX_ERR_STATE -5
#define FIX_ERR_BUSY -16
#define FIX_ERR_GONE -17

#define FIX_COLOR_RED 0
#define FIX_COLOR_GREEN 1
#define FIX_COLOR_BLUE 7

/* Another library's constant, which the prefix keeps out. */
#define OTHER_ERR_NULL -1

int32_t fix_color_count(uint32_t *out_count);

#endif
