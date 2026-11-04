export class FilterCoefficients {
    constructor() {
        this.a = [1.0];
        this.b = [1.0];
        this.order = 0;
    }
    linearGain0thOrder(linearGain) {
        this.b[0] = linearGain;
        this.order = 0;
    }
    lowPass1stOrderButterworth(cornerRadiansPerSample) {
        const g = 1.0 / Math.tan(cornerRadiansPerSample * 0.5);
        const a0 = 1.0 + g;
        this.a[1] = (1.0 - g) / a0;
        this.b[1] = this.b[0] = 1 / a0;
        this.order = 1;
    }
    lowPass1stOrderSimplified(cornerRadiansPerSample) {
        const g = 2.0 * Math.sin(cornerRadiansPerSample * 0.5);
        this.a[1] = g - 1.0;
        this.b[0] = g;
        this.b[1] = 0.0;
        this.order = 1;
    }
    highPass1stOrderButterworth(cornerRadiansPerSample) {
        const g = 1.0 / Math.tan(cornerRadiansPerSample * 0.5);
        const a0 = 1.0 + g;
        this.a[1] = (1.0 - g) / a0;
        this.b[0] = g / a0;
        this.b[1] = -g / a0;
        this.order = 1;
    }
    highShelf1stOrder(cornerRadiansPerSample, shelfLinearGain) {
        const tan = Math.tan(cornerRadiansPerSample * 0.5);
        const sqrtGain = Math.sqrt(shelfLinearGain);
        const g = (tan * sqrtGain - 1) / (tan * sqrtGain + 1.0);
        const a0 = 1.0;
        this.a[1] = g / a0;
        this.b[0] = (1.0 + g + shelfLinearGain * (1.0 - g)) / (2.0 * a0);
        this.b[1] = (1.0 + g - shelfLinearGain * (1.0 - g)) / (2.0 * a0);
        this.order = 1;
    }
    allPass1stOrderInvertPhaseAbove(cornerRadiansPerSample) {
        const g = (Math.sin(cornerRadiansPerSample) - 1.0) / Math.cos(cornerRadiansPerSample);
        this.a[1] = g;
        this.b[0] = g;
        this.b[1] = 1.0;
        this.order = 1;
    }
    allPass1stOrderFractionalDelay(delay) {
        const g = (1.0 - delay) / (1.0 + delay);
        this.a[1] = g;
        this.b[0] = g;
        this.b[1] = 1.0;
        this.order = 1;
    }
    lowPass2ndOrderButterworth(cornerRadiansPerSample, peakLinearGain) {
        const alpha = Math.sin(cornerRadiansPerSample) / (2.0 * peakLinearGain);
        const cos = Math.cos(cornerRadiansPerSample);
        const a0 = 1.0 + alpha;
        this.a[1] = -2.0 * cos / a0;
        this.a[2] = (1 - alpha) / a0;
        this.b[2] = this.b[0] = (1 - cos) / (2.0 * a0);
        this.b[1] = (1 - cos) / a0;
        this.order = 2;
    }
    lowPass2ndOrderSimplified(cornerRadiansPerSample, peakLinearGain) {
        const g = 2.0 * Math.sin(cornerRadiansPerSample / 2.0);
        const filterResonance = 1.0 - 1.0 / (2.0 * peakLinearGain);
        const feedback = filterResonance + filterResonance / (1.0 - g);
        this.a[1] = 2.0 * g + (g - 1.0) * g * feedback - 2.0;
        this.a[2] = (g - 1.0) * (g - g * feedback - 1.0);
        this.b[0] = g * g;
        this.b[1] = 0;
        this.b[2] = 0;
        this.order = 2;
    }
    highPass2ndOrderButterworth(cornerRadiansPerSample, peakLinearGain) {
        const alpha = Math.sin(cornerRadiansPerSample) / (2 * peakLinearGain);
        const cos = Math.cos(cornerRadiansPerSample);
        const a0 = 1.0 + alpha;
        this.a[1] = -2.0 * cos / a0;
        this.a[2] = (1.0 - alpha) / a0;
        this.b[2] = this.b[0] = (1.0 + cos) / (2.0 * a0);
        this.b[1] = -(1.0 + cos) / a0;
        this.order = 2;
    }
    highShelf2ndOrder(cornerRadiansPerSample, shelfLinearGain, slope) {
        const A = Math.sqrt(shelfLinearGain);
        const c = Math.cos(cornerRadiansPerSample);
        const Aplus = A + 1.0;
        const Aminus = A - 1.0;
        const alpha = Math.sin(cornerRadiansPerSample) * 0.5 * Math.sqrt((Aplus / A) * (1.0 / slope - 1.0) + 2.0);
        const sqrtA2Alpha = 2.0 * Math.sqrt(A) * alpha;
        const a0 = (Aplus - Aminus * c + sqrtA2Alpha);
        this.a[1] = 2 * (Aminus - Aplus * c) / a0;
        this.a[2] = (Aplus - Aminus * c - sqrtA2Alpha) / a0;
        this.b[0] = A * (Aplus + Aminus * c + sqrtA2Alpha) / a0;
        this.b[1] = -2 * A * (Aminus + Aplus * c) / a0;
        this.b[2] = A * (Aplus + Aminus * c - sqrtA2Alpha) / a0;
        this.order = 2;
    }
    peak2ndOrder(cornerRadiansPerSample, peakLinearGain, bandWidthScale) {
        const sqrtGain = Math.sqrt(peakLinearGain);
        const bandWidth = bandWidthScale * cornerRadiansPerSample / (sqrtGain >= 1 ? sqrtGain : 1 / sqrtGain);
        const alpha = Math.tan(bandWidth * 0.5);
        const a0 = 1.0 + alpha / sqrtGain;
        this.b[0] = (1.0 + alpha * sqrtGain) / a0;
        this.b[1] = this.a[1] = -2.0 * Math.cos(cornerRadiansPerSample) / a0;
        this.b[2] = (1.0 - alpha * sqrtGain) / a0;
        this.a[2] = (1.0 - alpha / sqrtGain) / a0;
        this.order = 2;
    }
}
export class FrequencyResponse {
    constructor() {
        this.real = 0.0;
        this.imag = 0.0;
        this.denom = 1.0;
    }
    analyze(filter, radiansPerSample) {
        this.analyzeComplex(filter, Math.cos(radiansPerSample), Math.sin(radiansPerSample));
    }
    analyzeComplex(filter, real, imag) {
        const a = filter.a;
        const b = filter.b;
        const realZ1 = real;
        const imagZ1 = -imag;
        let realNum = b[0] + b[1] * realZ1;
        let imagNum = b[1] * imagZ1;
        let realDenom = 1.0 + a[1] * realZ1;
        let imagDenom = a[1] * imagZ1;
        let realZ = realZ1;
        let imagZ = imagZ1;
        for (let i = 2; i <= filter.order; i++) {
            const realTemp = realZ * realZ1 - imagZ * imagZ1;
            const imagTemp = realZ * imagZ1 + imagZ * realZ1;
            realZ = realTemp;
            imagZ = imagTemp;
            realNum += b[i] * realZ;
            imagNum += b[i] * imagZ;
            realDenom += a[i] * realZ;
            imagDenom += a[i] * imagZ;
        }
        this.denom = realDenom * realDenom + imagDenom * imagDenom;
        this.real = realNum * realDenom + imagNum * imagDenom;
        this.imag = imagNum * realDenom - realNum * imagDenom;
    }
    magnitude() {
        return Math.sqrt(this.real * this.real + this.imag * this.imag) / this.denom;
    }
    angle() {
        return Math.atan2(this.imag, this.real);
    }
}
export class DynamicBiquadFilter {
    constructor() {
        this.a1 = 0.0;
        this.a2 = 0.0;
        this.b0 = 1.0;
        this.b1 = 0.0;
        this.b2 = 0.0;
        this.a1Delta = 0.0;
        this.a2Delta = 0.0;
        this.b0Delta = 0.0;
        this.b1Delta = 0.0;
        this.b2Delta = 0.0;
        this.output1 = 0.0;
        this.output2 = 0.0;
        this.useMultiplicativeInputCoefficients = false;
    }
    resetOutput() {
        this.output1 = 0.0;
        this.output2 = 0.0;
    }
    loadCoefficientsWithGradient(start, end, deltaRate, useMultiplicativeInputCoefficients) {
        if (start.order != 2 || end.order != 2)
            throw new Error();
        this.a1 = start.a[1];
        this.a2 = start.a[2];
        this.b0 = start.b[0];
        this.b1 = start.b[1];
        this.b2 = start.b[2];
        this.a1Delta = (end.a[1] - start.a[1]) * deltaRate;
        this.a2Delta = (end.a[2] - start.a[2]) * deltaRate;
        if (useMultiplicativeInputCoefficients) {
            this.b0Delta = Math.pow(end.b[0] / start.b[0], deltaRate);
            this.b1Delta = Math.pow(end.b[1] / start.b[1], deltaRate);
            this.b2Delta = Math.pow(end.b[2] / start.b[2], deltaRate);
        }
        else {
            this.b0Delta = (end.b[0] - start.b[0]) * deltaRate;
            this.b1Delta = (end.b[1] - start.b[1]) * deltaRate;
            this.b2Delta = (end.b[2] - start.b[2]) * deltaRate;
        }
        this.useMultiplicativeInputCoefficients = useMultiplicativeInputCoefficients;
    }
}
export function warpNyquistToInfinity(radians) {
    return 2.0 * Math.tan(radians * 0.5);
}
export function warpInfinityToNyquist(radians) {
    return 2.0 * Math.atan(radians * 0.5);
}
