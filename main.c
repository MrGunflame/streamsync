#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>

#include <vlc/vlc.h>
#include <vlc/libvlc_media.h>

typedef struct {
    void* ptr;
    size_t len;
} ring_buffer_t;

int main(int argc, char* argv[]) {
    printf("Hello World\n");

    ring_buffer_t buffer;

    new_ringbuf_from_file("/home/robert/Documents/Music/Stellaris/ridingthesolarwind.ogg", &buffer);

    libvlc_instance_t* instance;
    libvlc_media_player_t* player;
    libvlc_media_t* media;

    instance = libvlc_new(0, NULL);

    media = libvlc_media_new_path(instance, "/home/robert/Documents/Music/Stellaris/ridingthesolarwind.ogg");

    player = libvlc_media_player_new_from_media(media);

    libvlc_media_release(media);

    libvlc_media_player_play(player);

    printf("play\n");

    sleep(10);

    libvlc_media_player_stop(player);

    libvlc_media_player_release(player);

    libvlc_release(instance);
    return 0;
}

int new_ringbuf_from_file(char* path, ring_buffer_t* buf) {
    FILE* file = fopen(path, "r");
    if (file == NULL) {
        exit(1);
    }

    // 1 GB
    void* ptr = malloc(1000 * 1000 * 1000);
    if (ptr == NULL) {
        exit(1);
    }

    buf->ptr = ptr;

    char buffer[1000 * 1000 * 1000];

    size_t bytes_read = fread(buffer, 1000 * 1000 * 1000 + 1, 1, file);
    printf("Read %i\n", bytes_read);

    if (fclose(file)) {
        exit(1);
    }

    return 0;
}
