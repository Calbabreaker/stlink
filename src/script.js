const encoder = new TextEncoder();
const decoder = new TextDecoder();

// Encrypts a string with a password using AES-GCM
// Returns a string in the format encrypted_data,salt,iv
// See: https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto
async function encryptWithPassword(string, password) {
    const salt = crypto.getRandomValues(new Uint8Array(16));
    const key = await deriveKey(password, salt);

    // Generate a random Initialization Vector
    const iv = crypto.getRandomValues(new Uint8Array(12));

    // Encrypt the string using AES-GCM
    const cipherText = await crypto.subtle.encrypt(
        { name: "AES-GCM", iv },
        key,
        encoder.encode(string)
    );

    return [new Uint8Array(cipherText), salt, iv]
        .map((data) => btoa(String.fromCharCode(...data)))
        .join(",");
}

async function deriveKey(password, salt) {
    // Create an encryption key from the password using PBKDF2
    // See: https://developer.mozilla.org/en-US/docs/Web/API/SubtleCrypto
    const importedKey = await crypto.subtle.importKey(
        "raw",
        encoder.encode(password),
        "PBKDF2",
        false,
        ["deriveKey"]
    );

    return await crypto.subtle.deriveKey(
        {
            name: "PBKDF2",
            salt: encoder.encode(salt),
            iterations: 100000,
            hash: { name: "SHA-256" },
        },
        importedKey,
        { name: "AES-GCM", length: 256 },
        false,
        ["encrypt", "decrypt"]
    );
}

async function decryptWithPassword(rawData, password) {
    try {
        // Create the Uint8Array from the rawData
        // rawData is in the form of encrypted_data,salt,iv
        const [cipherText, salt, iv] = rawData.split(",").map(
            (section) =>
                new Uint8Array(
                    atob(section)
                        .split("")
                        .map((byte) => byte.charCodeAt(0))
                )
        );

        // Create the decryption key
        const key = await deriveKey(password, salt);

        // Decrypt the ciphertext using AES-GCM
        const decryptedData = await crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, cipherText);

        return decoder.decode(decryptedData);
    } catch (err) {
        console.error(err);
        return null;
    }
}

async function tryFetch(endpoint, method = "GET", body) {
    let options = {};
    if (body) {
        options = {
            headers: {
                "Content-Type": "application/json",
            },
            body: JSON.stringify(body),
        };
    }

    options.method = method;

    const res = await fetch(endpoint, options);
    if (res.ok) {
        return await res.text();
    } else {
        throw await res.text();
    }
}
