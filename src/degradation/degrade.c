#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <mastik/pda.h>
#include <mastik/util.h>
#include <mastik/symbol.h>

#include <sys/types.h>
#include <sys/wait.h>

#define BINARY "/home/development/Frodo/PQCrypto-LWEKE/frodo640/test_KEM"

int main(int ac, char **av)
{
    pda_t pda = pda_prepare();

    void *ptr = map_offset(BINARY, sym_getsymboloffset(BINARY, "store64"));

    if (ptr == NULL)
    {
        printf("Bad reference\n");
        exit(1);
    }

    pda_target(pda, ptr);

    printf("Running degradation: %p\n", ptr);
    pda_activate(pda);

    wait(NULL);
}
