#include <stdlib.h>

#include "page.c"

int main(int argc, char *argv[])
{
    int npages = argv[1] ? atoi(argv[1]) : 100;
    int show_all_pages = argc > 2 && strcmp(argv[2], "-a") == 0;
    testpage(npages, show_all_pages);
}
