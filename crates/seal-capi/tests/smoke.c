/* Built and run by tests/capi.rs when a C compiler is available. */
#include "seal.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(cond) do { if (!(cond)) { char e[256]; seal_last_error(e, sizeof e); \
    fprintf(stderr, "line %d: %s (%s)\n", __LINE__, #cond, e); return 1; } } while (0)

int main(void) {
    CHECK(seal_abi_version() == SEAL_ABI_VERSION);
    SealWorld *w = seal_world_new();
    CHECK(w != NULL);
    SealEntity root = seal_entity_spawn(w);
    SealEntity child = seal_entity_spawn(w);
    CHECK(seal_component_set(w, root, "name", "\"Root\"") == SEAL_OK);
    CHECK(seal_entity_set_parent(w, child, root) == SEAL_OK);
    CHECK(seal_entity_parent(w, child) == root);
    CHECK(seal_entity_set_parent(w, root, child) == SEAL_ERROR);

    size_t len = 0;
    CHECK(seal_component_get(w, root, "name", NULL, 0, &len) == SEAL_BUFFER_TOO_SMALL);
    char *buf = malloc(len + 1);
    CHECK(seal_component_get(w, root, "name", buf, len + 1, &len) == SEAL_OK);
    CHECK(strcmp(buf, "\"Root\"") == 0);
    free(buf);

    CHECK(seal_scene_save(w, NULL, 0, &len) == SEAL_BUFFER_TOO_SMALL);
    char *scene = malloc(len + 1);
    CHECK(seal_scene_save(w, scene, len + 1, &len) == SEAL_OK);
    SealWorld *copy = seal_world_new();
    CHECK(seal_scene_load(copy, scene) == SEAL_OK);
    CHECK(seal_world_entity_count(copy) == 2);
    free(scene);

    CHECK(seal_entity_despawn(w, child) == SEAL_OK);
    CHECK(!seal_entity_alive(w, child));
    CHECK(seal_entity_despawn(w, child) == SEAL_NO_SUCH_ENTITY);
    CHECK(seal_component_set(NULL, root, "name", "\"x\"") == SEAL_INVALID_ARGUMENT);
    seal_world_free(copy);
    seal_world_free(w);
    puts("c smoke ok");
    return 0;
}
