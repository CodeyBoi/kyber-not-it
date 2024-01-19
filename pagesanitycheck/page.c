// A simple program to test the pageattack procedure. It allocates a number of
// pages and then prints each PFN using the pagemap program. The output should
// contain the desired PFN in the correct spot.

#define PAGE_SIZE 4096

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int testpage(int npages, int show_all_pages)
{
    int pages[npages * PAGE_SIZE / sizeof(int)];
    for (int i = 0; i < npages; i++)
    {
        pages[i * PAGE_SIZE / sizeof(int)] = i + 1231;
    }

    int *first_addr = &pages[0];
    int *last_addr = &pages[npages * PAGE_SIZE / sizeof(int) - 1];

    if (show_all_pages)
    {
        system("sudo ./pagemap2 $$");
    }
    else
    {
        char cmd[120];
        snprintf(cmd, 120, "sudo ./pagemap %d %p %p", getpid(), first_addr, last_addr);
        system(cmd);
    }
}
