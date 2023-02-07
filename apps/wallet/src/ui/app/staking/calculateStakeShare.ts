import BigNumber from 'bignumber.js';

export const calculateStakeShare = (
    validatorStake: bigint,
    totalStake: bigint,
    decimalPlaces = 3
) => {
    const bn = new BigNumber(validatorStake.toString());
    const bd = new BigNumber(totalStake.toString());
    const percentage = bn
        .div(bd)
        .multipliedBy(100)
        .decimalPlaces(decimalPlaces)
        .toNumber();
    return percentage;
};
