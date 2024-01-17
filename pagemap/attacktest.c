#define PAGE_SIZE 4096

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <unistd.h>
#include <sys/mman.h>

int main(int argc, char *argv[])
{
    char cmd[120];
    int target_page = argv[1] ? atoi(argv[1]) : 5;

    int *pages = mmap(NULL, target_page * PAGE_SIZE, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (pages == MAP_FAILED)
    {
        perror("mmap");
        exit(1);
    }

    for (int i = 0; i < target_page; i++)
    {
        pages[i * PAGE_SIZE / sizeof(int)] = i;
    }

    snprintf(cmd, 120, "sudo ./pagemap %d %p %p", getpid(), &pages[0], &pages[target_page * PAGE_SIZE / sizeof(int) - 1]);
    system(cmd);
    snprintf(cmd, 120, "sudo ./pagetest %d", target_page * 2);
    printf("\n");

    for (int i = 0; i < target_page; i++)
    {
        munmap(&pages[i * PAGE_SIZE / sizeof(int)], PAGE_SIZE);
    }

    system(cmd);
}
