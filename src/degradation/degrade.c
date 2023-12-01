#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <mastik/pda.h>
#include <mastik/util.h>
#include <mastik/symbol.h>

#include <sys/types.h>
#include <sys/wait.h>

#define DEFAULT_BINARY "/home/development/Frodo/PQCrypto-LWEKE/frodo640/test_KEM"

int main(int argc, char **argv)
{
    int offset = 25;
    if (argc == 2) {
        offset = strtol(argv[1], 0, 10);
    }
    
    pda_t pda = pda_prepare();
    void *ptr = map_offset(DEFAULT_BINARY, sym_getsymboloffset(DEFAULT_BINARY, "store64") + offset);

    if (ptr == NULL)
    {
        printf("Bad reference, %s\n", DEFAULT_BINARY);
        exit(1);
    }

    pda_target(pda, ptr);

    printf("Running degradation: %p\n", ptr);
    pda_activate(pda);

    wait(NULL);
}
