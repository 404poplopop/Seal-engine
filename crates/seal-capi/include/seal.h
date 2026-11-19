/*
 * seal.h - the stable C interface to the Seal runtime.
 *
 * Link against seal_capi (static or shared). Every function is safe to call
 * with NULL handles and reports misuse with a status code instead of
 * crashing; Rust panics never cross this boundary.
 *
 * Strings in are NUL-terminated UTF-8. Strings out use the two-call pattern:
 * pass a buffer and its capacity; *len_out always receives the full length
 * (without the NUL). If the buffer was too small, SEAL_BUFFER_TOO_SMALL is
 * returned and nothing is written; call again with a bigger buffer.
 *
 * Component values are SealText, for example "{ x = 1, y = 2, z = 0 }".
 * A world handle must be used from one thread at a time.
 */
#ifndef SEAL_H
#define SEAL_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SEAL_ABI_VERSION 1u

typedef struct SealWorld SealWorld;

/* Generation in the high 32 bits, slot index in the low 32 bits. */
typedef uint64_t SealEntity;
#define SEAL_ENTITY_NONE UINT64_MAX

typedef enum SealStatus {
    SEAL_OK = 0,
    SEAL_ERROR = 1,              /* see seal_last_error */
    SEAL_BUFFER_TOO_SMALL = 2,
    SEAL_INVALID_ARGUMENT = 3,   /* NULL pointer or bad UTF-8 */
    SEAL_NO_SUCH_ENTITY = 4,
    SEAL_NOT_FOUND = 5,          /* the entity lacks that component */
    SEAL_PANIC = 6               /* internal bug; the world may be inconsistent */
} SealStatus;

uint32_t seal_abi_version(void);

/* Message for the last failure on this thread. Returns its full length. */
size_t seal_last_error(char *buf, size_t cap);

SealWorld *seal_world_new(void);
void seal_world_free(SealWorld *world);
size_t seal_world_entity_count(const SealWorld *world);

SealEntity seal_entity_spawn(SealWorld *world);
SealStatus seal_entity_despawn(SealWorld *world, SealEntity entity);
int seal_entity_alive(const SealWorld *world, SealEntity entity);
/* parent = SEAL_ENTITY_NONE detaches. Cycles are refused. */
SealStatus seal_entity_set_parent(SealWorld *world, SealEntity child, SealEntity parent);
SealEntity seal_entity_parent(const SealWorld *world, SealEntity child);

SealStatus seal_component_set(SealWorld *world, SealEntity entity,
                              const char *component, const char *sealtext);
SealStatus seal_component_get(const SealWorld *world, SealEntity entity,
                              const char *component, char *buf, size_t cap,
                              size_t *len_out);
SealStatus seal_component_remove(SealWorld *world, SealEntity entity,
                                 const char *component);

SealStatus seal_scene_save(const SealWorld *world, char *buf, size_t cap,
                           size_t *len_out);
/* Adds the scene's entities to the world; existing ones stay. */
SealStatus seal_scene_load(SealWorld *world, const char *sealtext);

#ifdef __cplusplus
}
#endif

#endif /* SEAL_H */
