export function scaleElementsByFactor(array, factor) {
    for (let i = 0; i < array.length; i++) {
        array[i] *= factor;
    }
}
function isPowerOf2(n) {
    return !!n && !(n & (n - 1));
}
function countBits(n) {
    if (!isPowerOf2(n))
        throw new Error("FFT array length must be a power of 2.");
    return Math.round(Math.log(n) / Math.log(2));
}
function reverseIndexBits(array, fullArrayLength) {
    const bitCount = countBits(fullArrayLength);
    const finalShift = 32 - bitCount;
    for (let i = 0; i < fullArrayLength; i++) {
        let j;
        j = ((i >> 1) & 0x55555555) | ((i & 0x55555555) << 1);
        j = ((j >> 2) & 0x33333333) | ((j & 0x33333333) << 2);
        j = ((j >> 4) & 0x0F0F0F0F) | ((j & 0x0F0F0F0F) << 4);
        j = ((j >> 8) & 0x00FF00FF) | ((j & 0x00FF00FF) << 8);
        j = ((j >> 16) & 0x0000FFFF) | ((j & 0x0000FFFF) << 16);
        j = j >>> finalShift;
        if (j > i) {
            let temp = array[i];
            array[i] = array[j];
            array[j] = temp;
        }
    }
}
export function discreteFourierTransform(realArray, imagArray) {
    const fullArrayLength = realArray.length;
    if (fullArrayLength != imagArray.length)
        throw new Error("FFT arrays must be the same length.");
    const realOut = [];
    const imagOut = [];
    for (let i = 0; i < fullArrayLength; i++) {
        realOut[i] = 0.0;
        imagOut[i] = 0.0;
        for (let j = 0; j < fullArrayLength; j++) {
            const radians = -Math.PI * 2.0 * j * i / fullArrayLength;
            const c = Math.cos(radians);
            const s = Math.sin(radians);
            realOut[i] += realArray[j] * c - imagArray[j] * s;
            imagOut[i] += realArray[j] * s + imagArray[j] * c;
        }
    }
    return [realOut, imagOut];
}
export function fastFourierTransform(realArray, imagArray) {
    const fullArrayLength = realArray.length;
    if (!isPowerOf2(fullArrayLength))
        throw new Error("FFT array length must be a power of 2.");
    if (fullArrayLength < 4)
        throw new Error("FFT array length must be at least 4.");
    if (fullArrayLength != imagArray.length)
        throw new Error("FFT arrays must be the same length.");
    reverseIndexBits(realArray, fullArrayLength);
    reverseIndexBits(imagArray, fullArrayLength);
    for (let startIndex = 0; startIndex < fullArrayLength; startIndex += 4) {
        const startIndex1 = startIndex + 1;
        const startIndex2 = startIndex + 2;
        const startIndex3 = startIndex + 3;
        const real0 = realArray[startIndex];
        const real1 = realArray[startIndex1];
        const real2 = realArray[startIndex2];
        const real3 = realArray[startIndex3];
        const imag0 = imagArray[startIndex];
        const imag1 = imagArray[startIndex1];
        const imag2 = imagArray[startIndex2];
        const imag3 = imagArray[startIndex3];
        const realTemp0 = real0 + real1;
        const realTemp1 = real0 - real1;
        const realTemp2 = real2 + real3;
        const realTemp3 = real2 - real3;
        const imagTemp0 = imag0 + imag1;
        const imagTemp1 = imag0 - imag1;
        const imagTemp2 = imag2 + imag3;
        const imagTemp3 = imag2 - imag3;
        realArray[startIndex] = realTemp0 + realTemp2;
        realArray[startIndex1] = realTemp1 + imagTemp3;
        realArray[startIndex2] = realTemp0 - realTemp2;
        realArray[startIndex3] = realTemp1 - imagTemp3;
        imagArray[startIndex] = imagTemp0 + imagTemp2;
        imagArray[startIndex1] = imagTemp1 - realTemp3;
        imagArray[startIndex2] = imagTemp0 - imagTemp2;
        imagArray[startIndex3] = imagTemp1 + realTemp3;
    }
    for (let stride = 8; stride <= fullArrayLength; stride += stride) {
        const halfLength = stride >>> 1;
        const radiansIncrement = Math.PI * 2.0 / stride;
        const cosIncrement = Math.cos(radiansIncrement);
        const sinIncrement = Math.sin(radiansIncrement);
        const oscillatorMultiplier = 2.0 * cosIncrement;
        for (let startIndex = 0; startIndex < fullArrayLength; startIndex += stride) {
            let c = 1.0;
            let s = 0.0;
            let cPrev = cosIncrement;
            let sPrev = sinIncrement;
            const secondHalf = startIndex + halfLength;
            for (let i = startIndex; i < secondHalf; i++) {
                const j = i + halfLength;
                const real0 = realArray[i];
                const imag0 = imagArray[i];
                const real1 = realArray[j] * c - imagArray[j] * s;
                const imag1 = realArray[j] * s + imagArray[j] * c;
                realArray[i] = real0 + real1;
                imagArray[i] = imag0 + imag1;
                realArray[j] = real0 - real1;
                imagArray[j] = imag0 - imag1;
                const cTemp = oscillatorMultiplier * c - cPrev;
                const sTemp = oscillatorMultiplier * s - sPrev;
                cPrev = c;
                sPrev = s;
                c = cTemp;
                s = sTemp;
            }
        }
    }
}
export function forwardRealFourierTransform(array) {
    const fullArrayLength = array.length;
    const totalPasses = countBits(fullArrayLength);
    if (fullArrayLength < 4)
        throw new Error("FFT array length must be at least 4.");
    reverseIndexBits(array, fullArrayLength);
    for (let index = 0; index < fullArrayLength; index += 4) {
        const index1 = index + 1;
        const index2 = index + 2;
        const index3 = index + 3;
        const real0 = array[index];
        const real1 = array[index1];
        const real2 = array[index2];
        const real3 = array[index3];
        const tempA = real0 + real1;
        const tempB = real2 + real3;
        array[index] = tempA + tempB;
        array[index1] = real0 - real1;
        array[index2] = tempA - tempB;
        array[index3] = real2 - real3;
    }
    const sqrt2over2 = Math.sqrt(2.0) / 2.0;
    for (let index = 0; index < fullArrayLength; index += 8) {
        const index1 = index + 1;
        const index3 = index + 3;
        const index4 = index + 4;
        const index5 = index + 5;
        const index7 = index + 7;
        const real0 = array[index];
        const real1 = array[index1];
        const imag3 = array[index3];
        const real4 = array[index4];
        const real5 = array[index5];
        const imag7 = array[index7];
        const tempA = (real5 - imag7) * sqrt2over2;
        const tempB = (real5 + imag7) * sqrt2over2;
        array[index] = real0 + real4;
        array[index1] = real1 + tempA;
        array[index3] = real1 - tempA;
        array[index4] = real0 - real4;
        array[index5] = tempB - imag3;
        array[index7] = tempB + imag3;
    }
    for (let pass = 3; pass < totalPasses; pass++) {
        const subStride = 1 << pass;
        const midSubStride = subStride >> 1;
        const stride = subStride << 1;
        const radiansIncrement = Math.PI * 2.0 / stride;
        const cosIncrement = Math.cos(radiansIncrement);
        const sinIncrement = Math.sin(radiansIncrement);
        const oscillatorMultiplier = 2.0 * cosIncrement;
        for (let startIndex = 0; startIndex < fullArrayLength; startIndex += stride) {
            const startIndexA = startIndex;
            const startIndexB = startIndexA + subStride;
            const stopIndex = startIndexB + subStride;
            const realStartA = array[startIndexA];
            const realStartB = array[startIndexB];
            array[startIndexA] = realStartA + realStartB;
            array[startIndexB] = realStartA - realStartB;
            let c = cosIncrement;
            let s = -sinIncrement;
            let cPrev = 1.0;
            let sPrev = 0.0;
            for (let index = 1; index < midSubStride; index++) {
                const indexA0 = startIndexA + index;
                const indexA1 = startIndexB - index;
                const indexB0 = startIndexB + index;
                const indexB1 = stopIndex - index;
                const real0 = array[indexA0];
                const imag0 = array[indexA1];
                const real1 = array[indexB0];
                const imag1 = array[indexB1];
                const tempA = real1 * c + imag1 * s;
                const tempB = real1 * s - imag1 * c;
                array[indexA0] = real0 + tempA;
                array[indexA1] = real0 - tempA;
                array[indexB0] = -imag0 - tempB;
                array[indexB1] = imag0 - tempB;
                const cTemp = oscillatorMultiplier * c - cPrev;
                const sTemp = oscillatorMultiplier * s - sPrev;
                cPrev = c;
                sPrev = s;
                c = cTemp;
                s = sTemp;
            }
        }
    }
}
export function inverseRealFourierTransform(array, fullArrayLength) {
    const totalPasses = countBits(fullArrayLength);
    if (fullArrayLength < 4)
        throw new Error("FFT array length must be at least 4.");
    for (let pass = totalPasses - 1; pass >= 2; pass--) {
        const subStride = 1 << pass;
        const midSubStride = subStride >> 1;
        const stride = subStride << 1;
        const radiansIncrement = Math.PI * 2.0 / stride;
        const cosIncrement = Math.cos(radiansIncrement);
        const sinIncrement = Math.sin(radiansIncrement);
        const oscillatorMultiplier = 2.0 * cosIncrement;
        for (let startIndex = 0; startIndex < fullArrayLength; startIndex += stride) {
            const startIndexA = startIndex;
            const midIndexA = startIndexA + midSubStride;
            const startIndexB = startIndexA + subStride;
            const midIndexB = startIndexB + midSubStride;
            const stopIndex = startIndexB + subStride;
            const realStartA = array[startIndexA];
            const imagStartB = array[startIndexB];
            array[startIndexA] = realStartA + imagStartB;
            array[midIndexA] *= 2;
            array[startIndexB] = realStartA - imagStartB;
            array[midIndexB] *= 2;
            let c = cosIncrement;
            let s = -sinIncrement;
            let cPrev = 1.0;
            let sPrev = 0.0;
            for (let index = 1; index < midSubStride; index++) {
                const indexA0 = startIndexA + index;
                const indexA1 = startIndexB - index;
                const indexB0 = startIndexB + index;
                const indexB1 = stopIndex - index;
                const real0 = array[indexA0];
                const real1 = array[indexA1];
                const imag0 = array[indexB0];
                const imag1 = array[indexB1];
                const tempA = real0 - real1;
                const tempB = imag0 + imag1;
                array[indexA0] = real0 + real1;
                array[indexA1] = imag1 - imag0;
                array[indexB0] = tempA * c - tempB * s;
                array[indexB1] = tempB * c + tempA * s;
                const cTemp = oscillatorMultiplier * c - cPrev;
                const sTemp = oscillatorMultiplier * s - sPrev;
                cPrev = c;
                sPrev = s;
                c = cTemp;
                s = sTemp;
            }
        }
    }
    for (let index = 0; index < fullArrayLength; index += 4) {
        const index1 = index + 1;
        const index2 = index + 2;
        const index3 = index + 3;
        const real0 = array[index];
        const real1 = array[index1] * 2;
        const imag2 = array[index2];
        const imag3 = array[index3] * 2;
        const tempA = real0 + imag2;
        const tempB = real0 - imag2;
        array[index] = tempA + real1;
        array[index1] = tempA - real1;
        array[index2] = tempB + imag3;
        array[index3] = tempB - imag3;
    }
    reverseIndexBits(array, fullArrayLength);
}
